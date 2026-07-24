use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use toml::Value as TomlValue;
use yaml_rust2::{Yaml, YamlLoader};

const WORKFLOW_PATH: &str = ".github/workflows/ci.yml";
const CANDIDATE_WORKFLOW_PATH: &str = ".github/workflows/reference-game-candidate.yml";
const REQUIRED_LOCKFILES: [&str; 3] = [
    "Cargo.lock",
    "reference-game/Cargo.lock",
    "module-consumer/Cargo.lock",
];
const REQUIRED_JOBS: [&str; 3] = ["root", "reference-game", "module-consumer"];
const ALLOWED_RUNNERS: [&str; 2] = ["ubuntu-latest", "windows-latest"];
const MAX_TIMEOUT_MINUTES: i64 = 45;
const CHECKOUT_ACTION: &str = "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0";
const INSTALL_ACTION: &str = "taiki-e/install-action@43aecc8d72668fbcfe75c31400bc4f890f1c5853";
const SETUP_PYTHON_ACTION: &str = "actions/setup-python@83679a892e2d95755f2dac6acb0bfd1e9ac5d548";
const UPLOAD_ARTIFACT_ACTION: &str =
    "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02";
const DOWNLOAD_ARTIFACT_ACTION: &str =
    "actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093";
const CONCURRENCY_GROUP: &str =
    "ci-${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}";
const CANDIDATE_CONCURRENCY_GROUP: &str =
    "reference-game-candidate-${{ github.workflow }}-${{ github.ref }}";
const CANDIDATE_JOB_GUARD: &str =
    "${{ github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' }}";
const EVIDENCE_INGEST_WORKFLOW_PATH: &str = ".github/workflows/reference-game-evidence-ingest.yml";
const EVIDENCE_INGEST_CONCURRENCY_GROUP: &str =
    "reference-game-evidence-ingest-${{ github.workflow }}-${{ github.ref }}";
const EVIDENCE_INGEST_JOB_GUARD: &str =
    "${{ github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' }}";
const GITHUB_SCRIPT_ACTION: &str = "actions/github-script@d746ffe35508b1917358783b479e04febd2b8f71";
const REVIEWED_EVIDENCE_SOURCE_REVISION: &str = "0c23f48d93ebb66c46377db30ce889943c0d2d63";
const EVIDENCE_INGEST_HELPER_PATH: &str = "reference-game/tools/ingest_evidence.py";
const EVIDENCE_INGEST_HELPER_BLOB: &str = "49a8604b4466c38d13884e67131198e744323150";
const EVIDENCE_INGEST_HELPER_SHA256: &str =
    "8fac0fbb409b622e0695e26e42fdc0db9698956881abc72805049068f16ae1f2";
const EVIDENCE_INGEST_SCHEMA_PATH: &str =
    "docs/benchmarks/data/envelope/v1/normalized-evidence.schema.json";
const EVIDENCE_INGEST_SCHEMA_BLOB: &str = "e6b22a9d818a3b7445396da5e8ea05a33348de06";
const EVIDENCE_INGEST_SCHEMA_SHA256: &str =
    "5ce8ff142febef0d8d844617622ff6ee9f6de4efc5718c2007b260786b17ee28";
const EVIDENCE_INGEST_POLICY_ROOT: &str = "${{ runner.temp }}/nara-evidence-ingest-policy";
const LINUX_STEP_GUARD: &str = "${{ runner.os == 'Linux' }}";
const WINDOWS_STEP_GUARD: &str = "${{ runner.os == 'Windows' }}";

#[test]
fn committed_ci_is_bounded_read_only_and_covers_three_locked_workspaces() {
    let fixture = PolicyFixture::committed();
    assert!(repository_root().join(WORKFLOW_PATH).is_file());
    for lockfile in REQUIRED_LOCKFILES {
        assert!(
            repository_root().join(lockfile).is_file(),
            "required independent lockfile is missing: {lockfile}"
        );
    }
    assert_policy_accepts(&fixture);
}

#[test]
fn committed_candidate_workflow_is_manual_read_only_and_has_a_no_checkout_consumer() {
    let fixture = CandidatePolicyFixture::committed();
    assert!(
        repository_root().join(CANDIDATE_WORKFLOW_PATH).is_file(),
        "candidate workflow is missing"
    );
    assert_candidate_policy_accepts(&fixture);
}

#[test]
fn committed_evidence_ingest_workflow_only_verifies_pinned_policy_material() {
    let fixture = EvidenceIngestPolicyFixture::committed();
    assert!(
        repository_root()
            .join(EVIDENCE_INGEST_WORKFLOW_PATH)
            .is_file(),
        "evidence-ingest workflow is missing"
    );
    assert_evidence_ingest_policy_accepts(&fixture);
}

#[test]
fn evidence_ingest_policy_rejects_untrusted_permissions_mutable_actions_and_execution() {
    let mut untrusted_trigger = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut untrusted_trigger.workflow,
        "  workflow_dispatch:\n",
        "  pull_request_target:\n",
    );
    assert_evidence_ingest_policy_rejects(&untrusted_trigger, "workflow_dispatch");

    let mut write_permission = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut write_permission.workflow,
        "contents: read",
        "contents: write",
    );
    assert_evidence_ingest_policy_rejects(&write_permission, "contents: read");

    let mut mutable_action = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut mutable_action.workflow,
        GITHUB_SCRIPT_ACTION,
        "actions/github-script@v9",
    );
    assert_evidence_ingest_policy_rejects(&mutable_action, "exact reviewed action");

    let mut checkout = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut checkout.workflow,
        "      - name: Fetch pinned helper and schema\n",
        concat!(
            "      - name: Checkout source\n",
            "        uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0\n",
            "      - name: Fetch pinned helper and schema\n",
        ),
    );
    assert_evidence_ingest_policy_rejects(&checkout, "exact three-step policy pipeline");

    let mut executes_candidate = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut executes_candidate.workflow,
        "verify-policy --schema",
        "normalize --schema",
    );
    assert_evidence_ingest_policy_rejects(&executes_candidate, "verify-policy");

    let mut leaks_token = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut leaks_token.workflow,
        &format!("          POLICY_ROOT: \"{EVIDENCE_INGEST_POLICY_ROOT}\""),
        &format!(
            "          POLICY_ROOT: \"{EVIDENCE_INGEST_POLICY_ROOT}\"\n          TOKEN: \"${{ github.token }}\""
        ),
    );
    assert_evidence_ingest_policy_rejects(&leaks_token, "github.token");

    let mut oidc = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut oidc.workflow,
        "  contents: read\n",
        "  contents: read\n  id-token: write\n",
    );
    assert_evidence_ingest_policy_rejects(&oidc, "OIDC");

    let mut repository_mutation = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut repository_mutation.workflow,
        "              fs.writeFileSync(path.join(policyRoot, file.output), bytes, { flag: \"wx\" });",
        "              exec(\"git push\");",
    );
    assert_evidence_ingest_policy_rejects(
        &repository_mutation,
        "executes or mutates outside policy verification",
    );

    let mut missing_guard = EvidenceIngestPolicyFixture::committed();
    replace_first(
        &mut missing_guard.workflow,
        &format!("    if: \"{EVIDENCE_INGEST_JOB_GUARD}\"\n"),
        "",
    );
    assert_evidence_ingest_policy_rejects(&missing_guard, "protected main dispatch");
}

#[test]
fn candidate_policy_rejects_untrusted_triggers_permissions_and_mutable_actions() {
    let mut untrusted_trigger = CandidatePolicyFixture::committed();
    replace_first(
        &mut untrusted_trigger.workflow,
        "  workflow_dispatch:\n",
        "  pull_request_target:\n",
    );
    assert_candidate_policy_rejects(&untrusted_trigger, "workflow_dispatch");

    let mut write_permission = CandidatePolicyFixture::committed();
    replace_first(
        &mut write_permission.workflow,
        "contents: read",
        "contents: write",
    );
    assert_candidate_policy_rejects(&write_permission, "contents: read");

    let mut mutable_action = CandidatePolicyFixture::committed();
    replace_first(
        &mut mutable_action.workflow,
        UPLOAD_ARTIFACT_ACTION,
        "actions/upload-artifact@v4",
    );
    assert_candidate_policy_rejects(&mutable_action, "exact reviewed action");

    let mut secret = CandidatePolicyFixture::committed();
    replace_first(
        &mut secret.workflow,
        "  CARGO_INCREMENTAL: \"0\"",
        "  CARGO_INCREMENTAL: \"0\"\n  TOKEN: \"${{ secrets.RELEASE_TOKEN }}\"",
    );
    assert_candidate_policy_rejects(&secret, "secret context");
}

#[test]
fn candidate_policy_rejects_checkout_in_consumer_and_missing_software_profile() {
    let mut consumer_checkout = CandidatePolicyFixture::committed();
    replace_first(
        &mut consumer_checkout.workflow,
        "      - name: Set up Python consumer",
        concat!(
            "      - name: Checkout consumer\n",
            "        uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0\n",
            "        with:\n",
            "          persist-credentials: false\n",
            "      - name: Set up Python consumer"
        ),
    );
    assert_candidate_policy_rejects(&consumer_checkout, "exact ordered candidate pipeline");

    let mut missing_linux_profile = CandidatePolicyFixture::committed();
    replace_first(
        &mut missing_linux_profile.workflow,
        " --desktop-environment NARA_WGPU_FORCE_FALLBACK=1",
        "",
    );
    assert_candidate_policy_rejects(&missing_linux_profile, "exact ordered candidate pipeline");

    let mut missing_guard = CandidatePolicyFixture::committed();
    replace_first(
        &mut missing_guard.workflow,
        &format!("    if: \"{CANDIDATE_JOB_GUARD}\"\n"),
        "",
    );
    assert_candidate_policy_rejects(&missing_guard, "protected main dispatch");
}

#[test]
fn policy_rejects_missing_lockfiles_and_dependency_shortcuts() {
    let mut missing_lock = PolicyFixture::committed();
    missing_lock.lockfiles.remove("module-consumer/Cargo.lock");
    assert_policy_rejects(&missing_lock, "missing lockfile");

    let mut private_dependency = PolicyFixture::committed();
    replace_first(
        &mut private_dependency.module_manifest,
        "\n[dev-dependencies]\n",
        "\nnara_app = { path = \"../crates/nara_app\" }\n\n[dev-dependencies]\n",
    );
    assert_policy_rejects(&private_dependency, "unexpected direct dependencies");

    let mut workspace_inheritance = PolicyFixture::committed();
    replace_first(
        &mut workspace_inheritance.module_manifest,
        "nara_scene = { path = \"../crates/nara_scene\", default-features = false, features = [\"serde\"] }",
        "nara_scene = { workspace = true }",
    );
    assert_policy_rejects(&workspace_inheritance, "workspace dependency inheritance");

    let mut patch_override = PolicyFixture::committed();
    patch_override
        .reference_manifest
        .push_str("\n[patch.crates-io]\nserde = { path = \"../serde\" }\n");
    assert_policy_rejects(&patch_override, "patch override");
}

#[test]
fn policy_rejects_mutable_actions_and_checkout_credentials() {
    let mut mutable_action = PolicyFixture::committed();
    replace_first(
        &mut mutable_action.workflow,
        CHECKOUT_ACTION,
        "actions/checkout@v7",
    );
    assert_policy_rejects(&mutable_action, "full commit SHA");

    let mut persistent_credentials = PolicyFixture::committed();
    replace_first(
        &mut persistent_credentials.workflow,
        "persist-credentials: false",
        "persist-credentials: true",
    );
    assert_policy_rejects(&persistent_credentials, "persist-credentials");
}

#[test]
fn policy_rejects_unbounded_or_uncancellable_jobs() {
    let mut missing_timeout = PolicyFixture::committed();
    replace_first(
        &mut missing_timeout.workflow,
        "    timeout-minutes: 45\n",
        "",
    );
    assert_policy_rejects(&missing_timeout, "timeout-minutes");

    let mut excessive_timeout = PolicyFixture::committed();
    replace_first(
        &mut excessive_timeout.workflow,
        "timeout-minutes: 45",
        "timeout-minutes: 120",
    );
    assert_policy_rejects(&excessive_timeout, "exceeds 45");

    let mut cancellation_disabled = PolicyFixture::committed();
    replace_first(
        &mut cancellation_disabled.workflow,
        "cancel-in-progress: true",
        "cancel-in-progress: false",
    );
    assert_policy_rejects(&cancellation_disabled, "cancel-in-progress");

    let mut trust_shared_group = PolicyFixture::committed();
    replace_first(
        &mut trust_shared_group.workflow,
        "ci-${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}",
        "ci-${{ github.workflow }}",
    );
    assert_policy_rejects(&trust_shared_group, "ref-scoped");
}

#[test]
fn policy_rejects_skipped_matrix_entries_and_masked_commands() {
    let mut skipped_job = PolicyFixture::committed();
    replace_first(
        &mut skipped_job.workflow,
        "    runs-on: \"${{ matrix.os }}\"\n",
        "    runs-on: \"${{ matrix.os }}\"\n    if: false\n",
    );
    assert_policy_rejects(&skipped_job, "forbidden key if");

    let mut skipped_step = PolicyFixture::committed();
    replace_first(
        &mut skipped_step.workflow,
        "      - name: Check root workspace\n",
        "      - name: Check root workspace\n        if: \"${{ false }}\"\n",
    );
    assert_policy_rejects(&skipped_step, "forbidden key if");

    let mut ignored_failure = PolicyFixture::committed();
    replace_first(
        &mut ignored_failure.workflow,
        "      - name: Check root workspace\n",
        "      - name: Check root workspace\n        continue-on-error: \"${{ true }}\"\n",
    );
    assert_policy_rejects(&ignored_failure, "forbidden key continue-on-error");

    let mut excluded_runner = PolicyFixture::committed();
    replace_first(
        &mut excluded_runner.workflow,
        "      matrix:\n        os:\n",
        "      matrix:\n        exclude:\n          - os: windows-latest\n        os:\n",
    );
    assert_policy_rejects(&excluded_runner, "workflow.jobs.root.strategy.matrix");

    let mut included_runner = PolicyFixture::committed();
    replace_first(
        &mut included_runner.workflow,
        "      matrix:\n        os:\n",
        "      matrix:\n        include:\n          - os: arc-runner-set\n        os:\n",
    );
    assert_policy_rejects(&included_runner, "workflow.jobs.root.strategy.matrix");

    let mut duplicated_runner = PolicyFixture::committed();
    replace_first(
        &mut duplicated_runner.workflow,
        "          - windows-latest\n",
        "          - windows-latest\n          - windows-latest\n",
    );
    assert_policy_rejects(&duplicated_runner, "exact hosted runner matrix");

    let mut non_string_runner = PolicyFixture::committed();
    replace_first(
        &mut non_string_runner.workflow,
        "          - windows-latest\n",
        "          - 7\n",
    );
    assert_policy_rejects(&non_string_runner, "exact hosted runner matrix");

    let mut masked_command = PolicyFixture::committed();
    replace_first(
        &mut masked_command.workflow,
        "cargo check --workspace --locked",
        "cargo check --workspace --locked || exit 0",
    );
    assert_policy_rejects(&masked_command, "exact ordered run commands");

    let mut echoed_command = PolicyFixture::committed();
    replace_first(
        &mut echoed_command.workflow,
        "run: cargo check --workspace --locked",
        "run: 'echo \"cargo check --workspace --locked\"'",
    );
    assert_policy_rejects(&echoed_command, "exact ordered run commands");
}

#[test]
fn policy_rejects_write_permission_secrets_and_oidc() {
    let mut write_permission = PolicyFixture::committed();
    replace_first(
        &mut write_permission.workflow,
        "contents: read",
        "contents: write",
    );
    assert_policy_rejects(&write_permission, "contents: read");

    let mut secret = PolicyFixture::committed();
    replace_first(
        &mut secret.workflow,
        "  CARGO_TERM_COLOR: always",
        "  CARGO_TERM_COLOR: always\n  PRIVATE_TOKEN: \"${{ secrets.PRIVATE_TOKEN }}\"",
    );
    assert_policy_rejects(&secret, "secret context");

    let mut github_token = PolicyFixture::committed();
    replace_first(
        &mut github_token.workflow,
        "      - name: Check root workspace\n",
        "      - name: Check root workspace\n        env:\n          TOKEN: \"${{ github.token }}\"\n",
    );
    assert_policy_rejects(&github_token, "github.token");

    let mut indexed_secret = PolicyFixture::committed();
    replace_first(
        &mut indexed_secret.workflow,
        "  CARGO_TERM_COLOR: always",
        "  CARGO_TERM_COLOR: always\n  PRIVATE_TOKEN: \"${{ secrets['PRIVATE_TOKEN'] }}\"",
    );
    assert_policy_rejects(&indexed_secret, "secret context");

    let mut all_secrets = PolicyFixture::committed();
    replace_first(
        &mut all_secrets.workflow,
        "  CARGO_TERM_COLOR: always",
        "  CARGO_TERM_COLOR: always\n  ALL_SECRETS: \"${{ toJSON(secrets) }}\"",
    );
    assert_policy_rejects(&all_secrets, "secret context");

    let mut oidc = PolicyFixture::committed();
    replace_first(
        &mut oidc.workflow,
        "  contents: read",
        "  contents: read\n  id-token: write",
    );
    assert_policy_rejects(&oidc, "OIDC");
}

#[test]
fn policy_rejects_persistent_runners_shared_caches_and_privileged_events() {
    let mut self_hosted = PolicyFixture::committed();
    replace_first(
        &mut self_hosted.workflow,
        "          - ubuntu-latest",
        "          - self-hosted",
    );
    assert_policy_rejects(&self_hosted, "hosted runner matrix");

    let mut shared_cache = PolicyFixture::committed();
    replace_first(
        &mut shared_cache.workflow,
        "      - name: Install Rust",
        "      - name: Shared cache\n        uses: actions/cache@0000000000000000000000000000000000000000\n        with:\n          path: target\n          key: shared\n      - name: Install Rust",
    );
    assert_policy_rejects(&shared_cache, "cache");

    let mut privileged_event = PolicyFixture::committed();
    replace_first(
        &mut privileged_event.workflow,
        "  pull_request:\n",
        "  pull_request:\n  pull_request_target:\n",
    );
    assert_policy_rejects(&privileged_event, "workflow.on");
}

#[derive(Clone)]
struct PolicyFixture {
    workflow: String,
    reference_manifest: String,
    module_manifest: String,
    lockfiles: BTreeSet<String>,
}

impl PolicyFixture {
    fn committed() -> Self {
        Self {
            workflow: include_str!("../.github/workflows/ci.yml").to_owned(),
            reference_manifest: include_str!("../reference-game/Cargo.toml").to_owned(),
            module_manifest: include_str!("../module-consumer/Cargo.toml").to_owned(),
            lockfiles: REQUIRED_LOCKFILES.into_iter().map(str::to_owned).collect(),
        }
    }

    fn violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        validate_workflow(&self.workflow, &mut violations);
        validate_manifest(
            "reference-game",
            &self.reference_manifest,
            &["dependencies.nara"],
            &mut violations,
        );
        validate_manifest(
            "module-consumer",
            &self.module_manifest,
            &[
                "dependencies.nara_reflect",
                "dependencies.nara_scene",
                "dev-dependencies.bevy_ecs",
            ],
            &mut violations,
        );
        for lockfile in REQUIRED_LOCKFILES {
            if !self.lockfiles.contains(lockfile) {
                violations.push(format!("missing lockfile {lockfile}"));
            }
        }
        violations.sort();
        violations.dedup();
        violations
    }
}

#[derive(Clone)]
struct CandidatePolicyFixture {
    workflow: String,
}

impl CandidatePolicyFixture {
    fn committed() -> Self {
        Self {
            workflow: fs::read_to_string(repository_root().join(CANDIDATE_WORKFLOW_PATH))
                .expect("candidate workflow must be readable"),
        }
    }

    fn violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        validate_candidate_workflow(&self.workflow, &mut violations);
        violations.sort();
        violations.dedup();
        violations
    }
}

#[derive(Clone)]
struct EvidenceIngestPolicyFixture {
    workflow: String,
}

impl EvidenceIngestPolicyFixture {
    fn committed() -> Self {
        Self {
            workflow: fs::read_to_string(repository_root().join(EVIDENCE_INGEST_WORKFLOW_PATH))
                .expect("evidence-ingest workflow must be readable"),
        }
    }

    fn violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        validate_evidence_ingest_workflow(&self.workflow, &mut violations);
        violations.sort();
        violations.dedup();
        violations
    }
}

fn validate_evidence_ingest_workflow(source: &str, violations: &mut Vec<String>) {
    let documents = match YamlLoader::load_from_str(source) {
        Ok(documents) => documents,
        Err(error) => {
            violations.push(format!(
                "evidence-ingest workflow is not valid YAML: {error}"
            ));
            return;
        }
    };
    if documents.len() != 1 {
        violations
            .push("evidence-ingest workflow must contain exactly one YAML document".to_owned());
        return;
    }
    let workflow = &documents[0];
    let Some(root) = workflow.as_hash() else {
        violations.push("evidence-ingest workflow root must be a mapping".to_owned());
        return;
    };
    validate_exact_keys(
        root,
        "evidence-ingest workflow",
        &["name", "on", "permissions", "concurrency", "jobs"],
        violations,
    );
    if scalar_str(field(root, "name")) != Some("Reference Game Evidence Ingest Preparation") {
        violations.push(
            "evidence-ingest workflow name must be exactly Reference Game Evidence Ingest Preparation"
                .to_owned(),
        );
    }
    validate_evidence_ingest_trigger(field(root, "on"), violations);
    validate_permissions(field(root, "permissions"), violations);
    validate_evidence_ingest_concurrency(field(root, "concurrency"), violations);
    validate_evidence_ingest_jobs(field(root, "jobs"), violations);
    scan_evidence_ingest_forbidden_features(workflow, "evidence-ingest workflow", violations);
}

fn validate_evidence_ingest_trigger(trigger: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(trigger) = trigger.and_then(Yaml::as_hash) else {
        violations.push("evidence-ingest workflow must declare workflow_dispatch only".to_owned());
        return;
    };
    validate_exact_keys(
        trigger,
        "evidence-ingest workflow.on",
        &["workflow_dispatch"],
        violations,
    );
    if !matches!(field(trigger, "workflow_dispatch"), Some(Yaml::Null)) {
        violations
            .push("evidence-ingest workflow_dispatch trigger must be unconfigured".to_owned());
    }
}

fn validate_evidence_ingest_concurrency(concurrency: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(concurrency) = concurrency.and_then(Yaml::as_hash) else {
        violations.push("evidence-ingest workflow concurrency is required".to_owned());
        return;
    };
    validate_exact_keys(
        concurrency,
        "evidence-ingest workflow.concurrency",
        &["group", "cancel-in-progress"],
        violations,
    );
    if scalar_str(field(concurrency, "group")) != Some(EVIDENCE_INGEST_CONCURRENCY_GROUP) {
        violations.push("evidence-ingest concurrency must be workflow- and ref-scoped".to_owned());
    }
    if field(concurrency, "cancel-in-progress").and_then(Yaml::as_bool) != Some(true) {
        violations.push("evidence-ingest concurrency must be cancellable".to_owned());
    }
}

fn validate_evidence_ingest_jobs(jobs: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(jobs) = jobs.and_then(Yaml::as_hash) else {
        violations.push("evidence-ingest workflow jobs must be a mapping".to_owned());
        return;
    };
    let expected = ["policy"]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if string_keys(jobs, "evidence-ingest workflow.jobs", violations) != expected {
        violations.push("evidence-ingest workflow must contain exactly one policy job".to_owned());
        return;
    }
    let Some(job) = field(jobs, "policy").and_then(Yaml::as_hash) else {
        return;
    };
    validate_exact_keys(
        job,
        "evidence-ingest workflow.jobs.policy",
        &["name", "if", "runs-on", "timeout-minutes", "steps"],
        violations,
    );
    if scalar_str(field(job, "name")) != Some("Verify pinned evidence-ingest policy") {
        violations.push("evidence-ingest policy job has the wrong display name".to_owned());
    }
    if scalar_str(field(job, "if")) != Some(EVIDENCE_INGEST_JOB_GUARD) {
        violations
            .push("evidence-ingest policy job must require a protected main dispatch".to_owned());
    }
    if scalar_str(field(job, "runs-on")) != Some("ubuntu-latest") {
        violations.push("evidence-ingest policy job must use ubuntu-latest".to_owned());
    }
    if field(job, "timeout-minutes").and_then(Yaml::as_i64) != Some(15) {
        violations.push("evidence-ingest policy job must have a 15 minute timeout".to_owned());
    }
    let Some(steps) = field(job, "steps").and_then(Yaml::as_vec) else {
        violations.push("evidence-ingest policy job must declare steps".to_owned());
        return;
    };
    validate_evidence_ingest_steps(steps, violations);
}

fn validate_evidence_ingest_steps(steps: &[Yaml], violations: &mut Vec<String>) {
    if steps.len() != 3 {
        violations.push(
            "evidence-ingest workflow must use an exact three-step policy pipeline".to_owned(),
        );
        return;
    }
    let Some(fetch_step) = steps[0].as_hash() else {
        violations.push("evidence-ingest fetch step must be a mapping".to_owned());
        return;
    };
    validate_exact_keys(
        fetch_step,
        "evidence-ingest workflow.jobs.policy.steps[0]",
        &["name", "uses", "env", "with"],
        violations,
    );
    if scalar_str(field(fetch_step, "name")) != Some("Fetch pinned helper and schema") {
        violations.push("evidence-ingest fetch step has the wrong name".to_owned());
    }
    if scalar_str(field(fetch_step, "uses")) != Some(GITHUB_SCRIPT_ACTION) {
        violations.push("evidence-ingest fetch step must use an exact reviewed action".to_owned());
    }
    validate_evidence_ingest_fetch_environment(field(fetch_step, "env"), violations);
    validate_evidence_ingest_fetch_script(field(fetch_step, "with"), violations);

    let Some(python_step) = steps[1].as_hash() else {
        violations.push("evidence-ingest Python setup step must be a mapping".to_owned());
        return;
    };
    validate_exact_keys(
        python_step,
        "evidence-ingest workflow.jobs.policy.steps[1]",
        &["name", "uses", "with"],
        violations,
    );
    if scalar_str(field(python_step, "name")) != Some("Set up Python") {
        violations.push("evidence-ingest Python setup step has the wrong name".to_owned());
    }
    if scalar_str(field(python_step, "uses")) != Some(SETUP_PYTHON_ACTION) {
        violations
            .push("evidence-ingest Python setup must use an exact reviewed action".to_owned());
    }
    validate_evidence_ingest_python_version(field(python_step, "with"), violations);

    let Some(verify_step) = steps[2].as_hash() else {
        violations.push("evidence-ingest verification step must be a mapping".to_owned());
        return;
    };
    validate_exact_keys(
        verify_step,
        "evidence-ingest workflow.jobs.policy.steps[2]",
        &["name", "run"],
        violations,
    );
    if scalar_str(field(verify_step, "name")) != Some("Verify pinned normalization policy") {
        violations.push("evidence-ingest verification step has the wrong name".to_owned());
    }
    let expected = format!(
        "python -B \"{EVIDENCE_INGEST_POLICY_ROOT}/ingest_evidence.py\" verify-policy \
         --schema \"{EVIDENCE_INGEST_POLICY_ROOT}/normalized-evidence.schema.json\""
    );
    if field(verify_step, "run")
        .and_then(Yaml::as_str)
        .map(normalize_run_script)
        .as_deref()
        != Some(expected.as_str())
    {
        violations.push(
            "evidence-ingest verification must run only the pinned verify-policy command"
                .to_owned(),
        );
    }
}

fn validate_evidence_ingest_fetch_environment(
    environment: Option<&Yaml>,
    violations: &mut Vec<String>,
) {
    let Some(environment) = environment.and_then(Yaml::as_hash) else {
        violations
            .push("evidence-ingest fetch step must declare its pinned environment".to_owned());
        return;
    };
    let expected = [
        "REVIEWED_SOURCE_REVISION",
        "HELPER_PATH",
        "HELPER_BLOB",
        "HELPER_SHA256",
        "SCHEMA_PATH",
        "SCHEMA_BLOB",
        "SCHEMA_SHA256",
        "POLICY_ROOT",
    ];
    validate_exact_keys(
        environment,
        "evidence-ingest workflow.jobs.policy.steps[0].env",
        &expected,
        violations,
    );
    for (key, expected) in [
        (
            "REVIEWED_SOURCE_REVISION",
            REVIEWED_EVIDENCE_SOURCE_REVISION,
        ),
        ("HELPER_PATH", EVIDENCE_INGEST_HELPER_PATH),
        ("HELPER_BLOB", EVIDENCE_INGEST_HELPER_BLOB),
        ("HELPER_SHA256", EVIDENCE_INGEST_HELPER_SHA256),
        ("SCHEMA_PATH", EVIDENCE_INGEST_SCHEMA_PATH),
        ("SCHEMA_BLOB", EVIDENCE_INGEST_SCHEMA_BLOB),
        ("SCHEMA_SHA256", EVIDENCE_INGEST_SCHEMA_SHA256),
        ("POLICY_ROOT", EVIDENCE_INGEST_POLICY_ROOT),
    ] {
        if scalar_str(field(environment, key)) != Some(expected) {
            violations.push(format!(
                "evidence-ingest fetch environment {key} is invalid"
            ));
        }
    }
}

fn validate_evidence_ingest_fetch_script(with: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(with) = with.and_then(Yaml::as_hash) else {
        violations.push("evidence-ingest fetch step must configure github-script".to_owned());
        return;
    };
    validate_exact_keys(
        with,
        "evidence-ingest workflow.jobs.policy.steps[0].with",
        &["script"],
        violations,
    );
    if field(with, "script")
        .and_then(Yaml::as_str)
        .map(normalize_run_script)
        .as_deref()
        != Some(normalize_run_script(EVIDENCE_INGEST_FETCH_SCRIPT).as_str())
    {
        violations.push(
            "evidence-ingest fetch step must use the exact reviewed read-only fetch script"
                .to_owned(),
        );
    }
}

fn validate_evidence_ingest_python_version(with: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(with) = with.and_then(Yaml::as_hash) else {
        violations.push("evidence-ingest Python setup must declare a version".to_owned());
        return;
    };
    validate_exact_keys(
        with,
        "evidence-ingest workflow.jobs.policy.steps[1].with",
        &["python-version"],
        violations,
    );
    if scalar_str(field(with, "python-version")) != Some("3.13.5") {
        violations.push("evidence-ingest Python setup must pin Python 3.13.5".to_owned());
    }
}

fn scan_evidence_ingest_forbidden_features(node: &Yaml, path: &str, violations: &mut Vec<String>) {
    match node {
        Yaml::Hash(mapping) => {
            for (key, value) in mapping {
                let Some(key) = key.as_str() else {
                    violations.push(format!("{path} contains a non-string key"));
                    continue;
                };
                let child_path = format!("{path}.{key}");
                let normalized = key.to_ascii_lowercase();
                if matches!(normalized.as_str(), "cache" | "cache-dependency-path") {
                    violations.push(format!("{child_path} introduces a shared cache"));
                }
                if normalized == "id-token" && scalar_str(Some(value)) != Some("none") {
                    violations.push(format!("{child_path} requests OIDC authority"));
                }
                if normalized == "continue-on-error" {
                    violations.push(format!("{child_path} masks failure"));
                }
                if normalized == "permissions" && path != "evidence-ingest workflow" {
                    violations.push(format!("{child_path} overrides workflow permissions"));
                }
                scan_evidence_ingest_forbidden_features(value, &child_path, violations);
            }
        }
        Yaml::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                scan_evidence_ingest_forbidden_features(
                    value,
                    &format!("{path}[{index}]"),
                    violations,
                );
            }
        }
        Yaml::String(value) => {
            let normalized = value.to_ascii_lowercase();
            if contains_ascii_identifier(&normalized, "secrets") {
                violations.push(format!("{path} references a secret context"));
            }
            if normalized.contains("github.token") {
                violations.push(format!("{path} references github.token"));
            }
            if normalized.contains("actions_id_token_request") {
                violations.push(format!("{path} requests OIDC environment state"));
            }
            if normalized.contains("pull_request_target") || normalized.contains("workflow_run") {
                violations.push(format!("{path} permits an untrusted trigger chain"));
            }
            if normalized.contains("actions/checkout@")
                || normalized.contains("actions/download-artifact@")
                || normalized.contains("actions/upload-artifact@")
            {
                violations.push(format!("{path} accesses a checkout or candidate artifact"));
            }
            if normalized.contains("actions/cache@")
                || normalized.contains("rust-cache")
                || normalized.contains("sccache")
            {
                violations.push(format!("{path} introduces a shared cache"));
            }
            if normalized == "self-hosted" {
                violations.push(format!("{path} uses a persistent self-hosted runner"));
            }
            if normalized.contains("cargo ")
                || normalized.contains("git checkout")
                || normalized.contains("git clone")
                || normalized.contains("git add")
                || normalized.contains("git commit")
                || normalized.contains("git push")
                || normalized.contains("child_process")
                || normalized.contains("exec(")
                || normalized.contains("spawn(")
                || normalized.contains("build-expectation")
                || normalized.contains(" normalize ")
            {
                violations.push(format!(
                    "{path} executes or mutates outside policy verification"
                ));
            }
        }
        Yaml::Alias(_) => violations.push(format!("{path} uses an uninspectable YAML alias")),
        _ => {}
    }
}

const EVIDENCE_INGEST_FETCH_SCRIPT: &str = r#"const crypto = require("crypto");
const fs = require("fs");
const path = require("path");

const owner = context.repo.owner;
const repo = context.repo.repo;
const reviewedRevision = process.env.REVIEWED_SOURCE_REVISION;
const policyRoot = process.env.POLICY_ROOT;
const files = [
  {
    path: process.env.HELPER_PATH,
    blob: process.env.HELPER_BLOB,
    sha256: process.env.HELPER_SHA256,
    output: "ingest_evidence.py",
  },
  {
    path: process.env.SCHEMA_PATH,
    blob: process.env.SCHEMA_BLOB,
    sha256: process.env.SCHEMA_SHA256,
    output: "normalized-evidence.schema.json",
  },
];

const branch = await github.rest.repos.getBranch({
  owner,
  repo,
  branch: "main",
});
if (!branch.data.protected || branch.data.commit.sha !== context.sha) {
  throw new Error("policy preparation requires the current protected main revision");
}

const comparison = await github.rest.repos.compareCommits({
  owner,
  repo,
  base: reviewedRevision,
  head: context.sha,
});
if (!["ahead", "identical"].includes(comparison.data.status)) {
  throw new Error("reviewed helper revision is not an ancestor of the dispatch revision");
}

fs.mkdirSync(policyRoot, { recursive: false });
for (const file of files) {
  const response = await github.rest.repos.getContent({
    owner,
    repo,
    path: file.path,
    ref: reviewedRevision,
  });
  if (
    Array.isArray(response.data) ||
    response.data.type !== "file" ||
    response.data.sha !== file.blob ||
    response.data.encoding !== "base64" ||
    typeof response.data.content !== "string"
  ) {
    throw new Error("reviewed policy source did not match its pinned blob");
  }

  const bytes = Buffer.from(response.data.content.replace(/\n/g, ""), "base64");
  const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
  if (sha256 !== file.sha256) {
    throw new Error("reviewed policy source did not match its pinned digest");
  }
  fs.writeFileSync(path.join(policyRoot, file.output), bytes, { flag: "wx" });
}"#;

fn validate_candidate_workflow(source: &str, violations: &mut Vec<String>) {
    let documents = match YamlLoader::load_from_str(source) {
        Ok(documents) => documents,
        Err(error) => {
            violations.push(format!("candidate workflow is not valid YAML: {error}"));
            return;
        }
    };
    if documents.len() != 1 {
        violations.push("candidate workflow must contain exactly one YAML document".to_owned());
        return;
    }
    let workflow = &documents[0];
    let Some(root) = workflow.as_hash() else {
        violations.push("candidate workflow root must be a mapping".to_owned());
        return;
    };
    validate_exact_keys(
        root,
        "candidate workflow",
        &["name", "on", "permissions", "concurrency", "env", "jobs"],
        violations,
    );
    if scalar_str(field(root, "name")) != Some("Reference Game Candidate") {
        violations
            .push("candidate workflow name must be exactly Reference Game Candidate".to_owned());
    }
    validate_candidate_trigger(field(root, "on"), violations);
    validate_permissions(field(root, "permissions"), violations);
    validate_candidate_concurrency(field(root, "concurrency"), violations);
    validate_environment(field(root, "env"), violations);
    validate_candidate_jobs(field(root, "jobs"), violations);
    scan_candidate_forbidden_features(workflow, "candidate workflow", violations);
}

fn validate_candidate_trigger(trigger: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(trigger) = trigger.and_then(Yaml::as_hash) else {
        violations.push("candidate workflow must declare workflow_dispatch only".to_owned());
        return;
    };
    validate_exact_keys(
        trigger,
        "candidate workflow.on",
        &["workflow_dispatch"],
        violations,
    );
    if !matches!(field(trigger, "workflow_dispatch"), Some(Yaml::Null)) {
        violations.push("candidate workflow_dispatch trigger must be unconfigured".to_owned());
    }
}

fn validate_candidate_concurrency(concurrency: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(concurrency) = concurrency.and_then(Yaml::as_hash) else {
        violations.push("candidate workflow concurrency is required".to_owned());
        return;
    };
    validate_exact_keys(
        concurrency,
        "candidate workflow.concurrency",
        &["group", "cancel-in-progress"],
        violations,
    );
    if scalar_str(field(concurrency, "group")) != Some(CANDIDATE_CONCURRENCY_GROUP) {
        violations.push("candidate concurrency must be workflow- and ref-scoped".to_owned());
    }
    if field(concurrency, "cancel-in-progress").and_then(Yaml::as_bool) != Some(true) {
        violations.push("candidate concurrency must be cancellable".to_owned());
    }
}

fn validate_candidate_jobs(jobs: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(jobs) = jobs.and_then(Yaml::as_hash) else {
        violations.push("candidate workflow jobs must be a mapping".to_owned());
        return;
    };
    let expected = ["candidate-build", "candidate-consumer"]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if string_keys(jobs, "candidate workflow.jobs", violations) != expected {
        violations
            .push("candidate workflow must contain exactly build and consumer jobs".to_owned());
    }
    for job_name in ["candidate-build", "candidate-consumer"] {
        let Some(job) = field(jobs, job_name).and_then(Yaml::as_hash) else {
            continue;
        };
        validate_candidate_job(job_name, job, violations);
    }
}

fn validate_candidate_job(
    job_name: &str,
    job: &yaml_rust2::yaml::Hash,
    violations: &mut Vec<String>,
) {
    let expected_keys = if job_name == "candidate-build" {
        vec![
            "name",
            "if",
            "runs-on",
            "timeout-minutes",
            "strategy",
            "steps",
        ]
    } else {
        vec![
            "name",
            "if",
            "needs",
            "runs-on",
            "timeout-minutes",
            "strategy",
            "steps",
        ]
    };
    validate_exact_keys(
        job,
        &format!("candidate workflow.jobs.{job_name}"),
        &expected_keys,
        violations,
    );
    let expected_name = if job_name == "candidate-build" {
        "Candidate build (${{ matrix.platform }})"
    } else {
        "Candidate consumer (${{ matrix.platform }})"
    };
    if scalar_str(field(job, "name")) != Some(expected_name) {
        violations.push(format!(
            "candidate job {job_name} has the wrong display name"
        ));
    }
    if scalar_str(field(job, "if")) != Some(CANDIDATE_JOB_GUARD) {
        violations.push(format!(
            "candidate job {job_name} must require a protected main dispatch"
        ));
    }
    if scalar_str(field(job, "runs-on")) != Some("${{ matrix.os }}") {
        violations.push(format!(
            "candidate job {job_name} must use its fixed runner matrix"
        ));
    }
    if field(job, "timeout-minutes").and_then(Yaml::as_i64) != Some(60) {
        violations.push(format!(
            "candidate job {job_name} must have a 60 minute timeout"
        ));
    }
    if job_name == "candidate-consumer"
        && scalar_str(field(job, "needs")) != Some("candidate-build")
    {
        violations.push("candidate consumer must depend on candidate-build".to_owned());
    }
    validate_candidate_matrix(job_name, field(job, "strategy"), violations);
    let Some(steps) = field(job, "steps").and_then(Yaml::as_vec) else {
        violations.push(format!("candidate job {job_name} must declare steps"));
        return;
    };
    validate_candidate_steps(job_name, steps, violations);
}

fn validate_candidate_matrix(
    job_name: &str,
    strategy: Option<&Yaml>,
    violations: &mut Vec<String>,
) {
    let Some(strategy) = strategy.and_then(Yaml::as_hash) else {
        violations.push(format!("candidate job {job_name} must declare a strategy"));
        return;
    };
    validate_exact_keys(
        strategy,
        &format!("candidate workflow.jobs.{job_name}.strategy"),
        &["fail-fast", "matrix"],
        violations,
    );
    if field(strategy, "fail-fast").and_then(Yaml::as_bool) != Some(false) {
        violations.push(format!(
            "candidate job {job_name} must set fail-fast: false"
        ));
    }
    let Some(matrix) = field(strategy, "matrix").and_then(Yaml::as_hash) else {
        violations.push(format!("candidate job {job_name} must declare a matrix"));
        return;
    };
    validate_exact_keys(
        matrix,
        &format!("candidate workflow.jobs.{job_name}.strategy.matrix"),
        &["include"],
        violations,
    );
    let observed = field(matrix, "include")
        .and_then(Yaml::as_vec)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let entry = entry.as_hash()?;
                    if string_keys(entry, "candidate matrix entry", violations)
                        != ["os", "platform", "binary_suffix"]
                            .into_iter()
                            .map(str::to_owned)
                            .collect()
                    {
                        violations.push("candidate matrix entry has unexpected keys".to_owned());
                    }
                    Some((
                        scalar_str(field(entry, "os"))?.to_owned(),
                        scalar_str(field(entry, "platform"))?.to_owned(),
                        scalar_str(field(entry, "binary_suffix"))?.to_owned(),
                    ))
                })
                .collect::<Vec<_>>()
        });
    let expected = vec![
        (
            "ubuntu-latest".to_owned(),
            "linux-x86_64".to_owned(),
            String::new(),
        ),
        (
            "windows-latest".to_owned(),
            "windows-x86_64".to_owned(),
            ".exe".to_owned(),
        ),
    ];
    if observed != Some(expected) {
        violations.push(format!(
            "candidate job {job_name} has the wrong platform matrix"
        ));
    }
}

fn validate_candidate_steps(job_name: &str, steps: &[Yaml], violations: &mut Vec<String>) {
    let mut pipeline = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let Some(step) = step.as_hash() else {
            violations.push(format!("candidate job {job_name} has a non-mapping step"));
            continue;
        };
        let path = format!("candidate workflow.jobs.{job_name}.steps[{index}]");
        let name = scalar_str(field(step, "name")).unwrap_or("");
        match (
            scalar_str(field(step, "uses")),
            scalar_str(field(step, "run")),
        ) {
            (Some(action), None) => {
                validate_exact_keys(step, &path, &["name", "uses", "with"], violations);
                validate_candidate_action(action, field(step, "with"), &path, violations);
                pipeline.push(format!("{name}|uses:{action}|if:"));
            }
            (None, Some(command)) => {
                let step_condition = scalar_str(field(step, "if")).unwrap_or("");
                let keys = if step_condition.is_empty() {
                    vec!["name", "run"]
                } else {
                    vec!["name", "if", "run"]
                };
                validate_exact_keys(step, &path, &keys, violations);
                pipeline.push(format!(
                    "{name}|run:{}|if:{step_condition}",
                    normalize_run_script(command)
                ));
            }
            _ => violations.push(format!(
                "candidate job {job_name} step {index} must declare exactly one of uses or run"
            )),
        }
    }
    let expected_pipeline = expected_candidate_pipeline(job_name);
    if pipeline != expected_pipeline {
        violations.push(format!(
            "candidate job {job_name} must use the exact ordered candidate pipeline; \
             observed {pipeline:#?}; expected {expected_pipeline:#?}"
        ));
    }
}

fn validate_candidate_action(
    action: &str,
    with: Option<&Yaml>,
    path: &str,
    violations: &mut Vec<String>,
) {
    if !matches!(
        action,
        CHECKOUT_ACTION | SETUP_PYTHON_ACTION | UPLOAD_ARTIFACT_ACTION | DOWNLOAD_ARTIFACT_ACTION
    ) {
        violations.push(format!("{path} must use an exact reviewed action"));
        return;
    }
    let Some(with) = with.and_then(Yaml::as_hash) else {
        violations.push(format!("{path} action must declare with"));
        return;
    };
    match action {
        CHECKOUT_ACTION => {
            validate_exact_keys(
                with,
                &format!("{path}.with"),
                &["persist-credentials"],
                violations,
            );
            if field(with, "persist-credentials").and_then(Yaml::as_bool) != Some(false) {
                violations.push(format!(
                    "{path} checkout must disable persisted credentials"
                ));
            }
        }
        SETUP_PYTHON_ACTION => {
            validate_exact_keys(
                with,
                &format!("{path}.with"),
                &["python-version"],
                violations,
            );
            if scalar_str(field(with, "python-version")) != Some("3.13.5") {
                violations.push(format!("{path} must pin Python 3.13.5"));
            }
        }
        UPLOAD_ARTIFACT_ACTION => {
            validate_exact_keys(
                with,
                &format!("{path}.with"),
                &[
                    "name",
                    "path",
                    "if-no-files-found",
                    "retention-days",
                    "compression-level",
                    "include-hidden-files",
                ],
                violations,
            );
            for (key, expected) in [
                (
                    "name",
                    "nara-reference-game-${{ matrix.platform }}-${{ github.run_id }}-${{ github.run_attempt }}",
                ),
                (
                    "path",
                    "${{ runner.temp }}/candidate-bundle-${{ matrix.platform }}",
                ),
                ("if-no-files-found", "error"),
                ("retention-days", "14"),
                ("compression-level", "0"),
                ("include-hidden-files", "false"),
            ] {
                if scalar_str(field(with, key)) != Some(expected) {
                    violations.push(format!("{path} upload field {key} is invalid"));
                }
            }
        }
        DOWNLOAD_ARTIFACT_ACTION => {
            validate_exact_keys(with, &format!("{path}.with"), &["name", "path"], violations);
            for (key, expected) in [
                (
                    "name",
                    "nara-reference-game-${{ matrix.platform }}-${{ github.run_id }}-${{ github.run_attempt }}",
                ),
                ("path", "${{ runner.temp }}/candidate-bundle"),
            ] {
                if scalar_str(field(with, key)) != Some(expected) {
                    violations.push(format!("{path} download field {key} is invalid"));
                }
            }
        }
        _ => {}
    }
}

fn expected_candidate_pipeline(job_name: &str) -> Vec<String> {
    match job_name {
        "candidate-build" => vec![
            format!("Checkout source|uses:{CHECKOUT_ACTION}|if:"),
            format!("Set up Python packager|uses:{SETUP_PYTHON_ACTION}|if:"),
            "Install Rust|run:rustup toolchain install 1.95.0 --profile minimal\nrustup default 1.95.0|if:".to_owned(),
            "Build headless product|run:cargo build --manifest-path reference-game/Cargo.toml --locked --release --bin headless|if:".to_owned(),
            "Build desktop product and probe|run:cargo build --manifest-path reference-game/Cargo.toml --locked --release --features desktop --bin desktop --bin desktop_render_probe|if:".to_owned(),
            concat!(
                "Create bounded candidate|run:python -B reference-game/tools/package.py create --repository-root . ",
                "--platform \"${{ matrix.platform }}\" --version 0.1.0 --source-revision \"${{ github.sha }}\" ",
                "--headless-binary \"reference-game/target/release/headless${{ matrix.binary_suffix }}\" ",
                "--desktop-binary \"reference-game/target/release/desktop${{ matrix.binary_suffix }}\" ",
                "--desktop-probe-binary \"reference-game/target/release/desktop_render_probe${{ matrix.binary_suffix }}\" ",
                "--output \"${{ runner.temp }}/nara-reference-game-${{ matrix.platform }}.zip\" ",
                "--receipt \"${{ runner.temp }}/nara-reference-game-${{ matrix.platform }}.json\"|if:"
            )
            .to_owned(),
            concat!(
                "Create no-checkout transport|run:python -B reference-game/tools/package.py bundle --repository-root . ",
                "--archive \"${{ runner.temp }}/nara-reference-game-${{ matrix.platform }}.zip\" ",
                "--receipt \"${{ runner.temp }}/nara-reference-game-${{ matrix.platform }}.json\" ",
                "--output \"${{ runner.temp }}/candidate-bundle-${{ matrix.platform }}\"|if:"
            )
            .to_owned(),
            format!("Upload candidate transport|uses:{UPLOAD_ARTIFACT_ACTION}|if:"),
        ],
        "candidate-consumer" => vec![
            format!("Set up Python consumer|uses:{SETUP_PYTHON_ACTION}|if:"),
            format!("Download exact candidate transport|uses:{DOWNLOAD_ARTIFACT_ACTION}|if:"),
            concat!(
                "Verify candidate transport before extraction|run:python -B ",
                "\"${{ runner.temp }}/candidate-bundle/verification/reference-game/tools/smoke_artifact.py\" ",
                "bundle-verify --bundle \"${{ runner.temp }}/candidate-bundle\" ",
                "--expected-platform \"${{ matrix.platform }}\" --expected-source-revision \"${{ github.sha }}\"|if:"
            )
            .to_owned(),
            format!(
                "Install Linux software display and Vulkan fallback|run:sudo apt-get update\nsudo apt-get install --yes xvfb mesa-vulkan-drivers vulkan-tools|if:{LINUX_STEP_GUARD}"
            ),
            format!(
                "{}|if:{LINUX_STEP_GUARD}",
                concat!(
                    "Smoke Linux candidate|run:python -B ",
                    "\"${{ runner.temp }}/candidate-bundle/verification/reference-game/tools/smoke_artifact.py\" ",
                    "bundle-smoke --bundle \"${{ runner.temp }}/candidate-bundle\" --work-root \"${{ runner.temp }}\" ",
                    "--expected-platform \"${{ matrix.platform }}\" --expected-source-revision \"${{ github.sha }}\" ",
                    "--desktop-launcher-json '[\"xvfb-run\",\"--auto-servernum\",\"--server-args=-screen 0 1280x720x24\"]' ",
                    "--desktop-environment NARA_WGPU_FORCE_FALLBACK=1"
                )
            ),
            format!(
                "{}|if:{WINDOWS_STEP_GUARD}",
                concat!(
                    "Smoke Windows candidate|run:python -B ",
                    "\"${{ runner.temp }}/candidate-bundle/verification/reference-game/tools/smoke_artifact.py\" ",
                    "bundle-smoke --bundle \"${{ runner.temp }}/candidate-bundle\" --work-root \"${{ runner.temp }}\" ",
                    "--expected-platform \"${{ matrix.platform }}\" --expected-source-revision \"${{ github.sha }}\" ",
                    "--desktop-environment NARA_WGPU_FORCE_FALLBACK=1"
                )
            ),
        ],
        _ => Vec::new(),
    }
}

fn scan_candidate_forbidden_features(node: &Yaml, path: &str, violations: &mut Vec<String>) {
    match node {
        Yaml::Hash(mapping) => {
            for (key, value) in mapping {
                let Some(key) = key.as_str() else {
                    violations.push(format!("{path} contains a non-string key"));
                    continue;
                };
                let child_path = format!("{path}.{key}");
                let normalized = key.to_ascii_lowercase();
                if matches!(normalized.as_str(), "cache" | "cache-dependency-path") {
                    violations.push(format!("{child_path} introduces a shared cache"));
                }
                if normalized == "id-token" && scalar_str(Some(value)) != Some("none") {
                    violations.push(format!("{child_path} requests OIDC authority"));
                }
                if normalized == "continue-on-error" {
                    violations.push(format!("{child_path} masks failure"));
                }
                if normalized == "permissions" && path != "candidate workflow" {
                    violations.push(format!("{child_path} overrides workflow permissions"));
                }
                scan_candidate_forbidden_features(value, &child_path, violations);
            }
        }
        Yaml::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                scan_candidate_forbidden_features(value, &format!("{path}[{index}]"), violations);
            }
        }
        Yaml::String(value) => {
            let normalized = value.to_ascii_lowercase();
            if contains_ascii_identifier(&normalized, "secrets") {
                violations.push(format!("{path} references a secret context"));
            }
            if normalized.contains("github.token") {
                violations.push(format!("{path} references github.token"));
            }
            if normalized.contains("actions_id_token_request") {
                violations.push(format!("{path} requests OIDC environment state"));
            }
            if normalized.contains("pull_request_target") || normalized.contains("workflow_run") {
                violations.push(format!("{path} permits an untrusted trigger chain"));
            }
            if normalized.contains("actions/cache@")
                || normalized.contains("rust-cache")
                || normalized.contains("sccache")
            {
                violations.push(format!("{path} introduces a shared cache"));
            }
            if normalized == "self-hosted" {
                violations.push(format!("{path} uses a persistent self-hosted runner"));
            }
        }
        Yaml::Alias(_) => violations.push(format!("{path} uses an uninspectable YAML alias")),
        _ => {}
    }
}

fn validate_workflow(source: &str, violations: &mut Vec<String>) {
    let documents = match YamlLoader::load_from_str(source) {
        Ok(documents) => documents,
        Err(error) => {
            violations.push(format!("workflow is not valid YAML: {error}"));
            return;
        }
    };
    if documents.len() != 1 {
        violations.push("workflow must contain exactly one YAML document".to_owned());
        return;
    }
    let workflow = &documents[0];
    let Some(root) = workflow.as_hash() else {
        violations.push("workflow root must be a mapping".to_owned());
        return;
    };

    validate_exact_keys(
        root,
        "workflow",
        &["name", "on", "permissions", "concurrency", "env", "jobs"],
        violations,
    );
    if scalar_str(field(root, "name")) != Some("CI") {
        violations.push("workflow name must be exactly CI".to_owned());
    }
    validate_triggers(field(root, "on"), violations);
    validate_permissions(field(root, "permissions"), violations);
    validate_concurrency(field(root, "concurrency"), violations);
    validate_environment(field(root, "env"), violations);
    validate_jobs(field(root, "jobs"), violations);
    scan_forbidden_workflow_features(workflow, "workflow", violations);
}

fn validate_triggers(trigger: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(trigger) = trigger.and_then(Yaml::as_hash) else {
        violations.push("workflow must declare pull_request and push triggers".to_owned());
        return;
    };
    validate_exact_keys(
        trigger,
        "workflow.on",
        &["pull_request", "push"],
        violations,
    );
    if !matches!(field(trigger, "pull_request"), Some(Yaml::Null)) {
        violations.push("workflow pull_request trigger must be unconfigured".to_owned());
    }
    let Some(push) = field(trigger, "push").and_then(Yaml::as_hash) else {
        violations.push("workflow push trigger must target main".to_owned());
        return;
    };
    validate_exact_keys(push, "workflow.on.push", &["branches"], violations);
    let branches = field(push, "branches")
        .and_then(Yaml::as_vec)
        .and_then(|branches| {
            branches
                .iter()
                .map(Yaml::as_str)
                .collect::<Option<Vec<_>>>()
        });
    if branches != Some(vec!["main"]) {
        violations.push("workflow push trigger must target only main".to_owned());
    }
}

fn validate_permissions(permissions: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(permissions) = permissions.and_then(Yaml::as_hash) else {
        violations.push("workflow permissions must be contents: read".to_owned());
        return;
    };
    if permissions.len() != 1 || scalar_str(field(permissions, "contents")) != Some("read") {
        violations.push("workflow permissions must be exactly contents: read".to_owned());
    }
}

fn validate_concurrency(concurrency: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(concurrency) = concurrency.and_then(Yaml::as_hash) else {
        violations.push("workflow concurrency is required".to_owned());
        return;
    };
    validate_exact_keys(
        concurrency,
        "workflow.concurrency",
        &["group", "cancel-in-progress"],
        violations,
    );
    if scalar_str(field(concurrency, "group")) != Some(CONCURRENCY_GROUP) {
        violations.push("concurrency group must be workflow- and ref-scoped".to_owned());
    }
    if field(concurrency, "cancel-in-progress").and_then(Yaml::as_bool) != Some(true) {
        violations.push("concurrency must set cancel-in-progress: true".to_owned());
    }
}

fn validate_environment(environment: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(environment) = environment.and_then(Yaml::as_hash) else {
        violations.push("workflow must declare bounded Cargo environment defaults".to_owned());
        return;
    };
    validate_exact_keys(
        environment,
        "workflow.env",
        &["CARGO_BUILD_JOBS", "CARGO_INCREMENTAL", "CARGO_TERM_COLOR"],
        violations,
    );
    for (key, expected) in [
        ("CARGO_BUILD_JOBS", "1"),
        ("CARGO_INCREMENTAL", "0"),
        ("CARGO_TERM_COLOR", "always"),
    ] {
        if scalar_str(field(environment, key)) != Some(expected) {
            violations.push(format!("workflow env {key} must equal {expected}"));
        }
    }
}

fn validate_jobs(jobs: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(jobs) = jobs.and_then(Yaml::as_hash) else {
        violations.push("workflow jobs must be a mapping".to_owned());
        return;
    };
    let observed = string_keys(jobs, "workflow.jobs", violations);
    let expected = REQUIRED_JOBS
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if observed != expected {
        violations.push(format!("workflow must contain exactly {expected:?}"));
    }

    for job_name in REQUIRED_JOBS {
        let Some(job) = field(jobs, job_name).and_then(Yaml::as_hash) else {
            continue;
        };
        validate_job(job_name, job, violations);
    }
}

fn validate_job(job_name: &str, job: &yaml_rust2::yaml::Hash, violations: &mut Vec<String>) {
    validate_exact_keys(
        job,
        &format!("workflow.jobs.{job_name}"),
        &["name", "runs-on", "timeout-minutes", "strategy", "steps"],
        violations,
    );
    let expected_name = match job_name {
        "root" => "Root (${{ matrix.os }})",
        "reference-game" => "Reference game (${{ matrix.os }})",
        "module-consumer" => "Module consumer (${{ matrix.os }})",
        _ => "",
    };
    if scalar_str(field(job, "name")) != Some(expected_name) {
        violations.push(format!(
            "job {job_name} must use its canonical display name"
        ));
    }
    let timeout = field(job, "timeout-minutes").and_then(Yaml::as_i64);
    if !timeout.is_some_and(|minutes| (1..=MAX_TIMEOUT_MINUTES).contains(&minutes)) {
        violations.push(format!(
            "job {job_name} timeout-minutes is missing or exceeds {MAX_TIMEOUT_MINUTES}"
        ));
    }
    if scalar_str(field(job, "runs-on")) != Some("${{ matrix.os }}") {
        violations.push(format!(
            "job {job_name} must use the declared hosted runner matrix"
        ));
    }

    let Some(strategy) = field(job, "strategy").and_then(Yaml::as_hash) else {
        violations.push(format!("job {job_name} must declare a strategy"));
        return;
    };
    validate_exact_keys(
        strategy,
        &format!("workflow.jobs.{job_name}.strategy"),
        &["fail-fast", "matrix"],
        violations,
    );
    if field(strategy, "fail-fast").and_then(Yaml::as_bool) != Some(false) {
        violations.push(format!("job {job_name} must set fail-fast: false"));
    }
    let Some(matrix) = field(strategy, "matrix").and_then(Yaml::as_hash) else {
        violations.push(format!("job {job_name} must declare a runner matrix"));
        return;
    };
    validate_exact_keys(
        matrix,
        &format!("workflow.jobs.{job_name}.strategy.matrix"),
        &["os"],
        violations,
    );
    let runners = field(matrix, "os")
        .and_then(Yaml::as_vec)
        .and_then(|values| values.iter().map(Yaml::as_str).collect::<Option<Vec<_>>>());
    if runners.as_deref() != Some(ALLOWED_RUNNERS.as_slice()) {
        violations.push(format!(
            "job {job_name} must use the exact hosted runner matrix {ALLOWED_RUNNERS:?}"
        ));
    }

    let Some(steps) = field(job, "steps").and_then(Yaml::as_vec) else {
        violations.push(format!("job {job_name} must declare steps"));
        return;
    };
    validate_job_steps(job_name, steps, violations);
}

fn validate_job_steps(job_name: &str, steps: &[Yaml], violations: &mut Vec<String>) {
    let mut checkout_count = 0;
    let mut install_count = 0;
    let mut pipeline = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let Some(step) = step.as_hash() else {
            violations.push(format!("job {job_name} contains a non-mapping step"));
            continue;
        };
        let step_path = format!("workflow.jobs.{job_name}.steps[{index}]");
        match (
            scalar_str(field(step, "uses")),
            scalar_str(field(step, "run")),
        ) {
            (Some(action), None) => {
                validate_exact_keys(step, &step_path, &["name", "uses", "with"], violations);
                validate_action_pin(action, job_name, violations);
                pipeline.push(format!("uses:{action}"));
                let Some(with) = field(step, "with").and_then(Yaml::as_hash) else {
                    violations.push(format!(
                        "job {job_name} action {action:?} must declare with"
                    ));
                    continue;
                };
                if action == CHECKOUT_ACTION {
                    validate_exact_keys(
                        with,
                        &format!("{step_path}.with"),
                        &["persist-credentials"],
                        violations,
                    );
                    checkout_count += 1;
                    let credentials = field(with, "persist-credentials").and_then(Yaml::as_bool);
                    if credentials != Some(false) {
                        violations.push(format!(
                            "job {job_name} checkout must set persist-credentials: false"
                        ));
                    }
                } else if action == INSTALL_ACTION {
                    validate_exact_keys(
                        with,
                        &format!("{step_path}.with"),
                        &["tool", "fallback"],
                        violations,
                    );
                    install_count += 1;
                    let tool = field(with, "tool").and_then(Yaml::as_str);
                    let fallback = field(with, "fallback").and_then(Yaml::as_str);
                    if tool != Some("nextest@0.9.138") || fallback != Some("none") {
                        violations.push(format!(
                            "job {job_name} must pin nextest 0.9.138 with no fallback"
                        ));
                    }
                }
            }
            (None, Some(run)) => {
                validate_exact_keys(step, &step_path, &["name", "run"], violations);
                if run.contains("${{") {
                    violations.push(format!(
                        "job {job_name} run commands must not interpolate GitHub contexts"
                    ));
                }
                pipeline.push(format!("run:{}", normalize_run_script(run)));
            }
            _ => violations.push(format!(
                "job {job_name} step {index} must declare exactly one of uses or run"
            )),
        }
    }
    if checkout_count != 1 {
        violations.push(format!(
            "job {job_name} must use pinned checkout exactly once"
        ));
    }
    if install_count != 1 {
        violations.push(format!(
            "job {job_name} must use pinned nextest installer exactly once"
        ));
    }
    let expected = required_step_pipeline(job_name);
    if pipeline.iter().map(String::as_str).collect::<Vec<_>>() != expected {
        violations.push(format!(
            "job {job_name} must use the exact ordered run commands and actions"
        ));
    }
}

fn required_step_pipeline(job_name: &str) -> &'static [&'static str] {
    match job_name {
        "root" => &[
            "uses:actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
            "run:rustup toolchain install 1.95.0 --profile minimal --component rustfmt\nrustup default 1.95.0",
            "uses:taiki-e/install-action@43aecc8d72668fbcfe75c31400bc4f890f1c5853",
            "run:cargo fmt --all -- --check",
            "run:cargo check --workspace --locked",
            "run:cargo nextest run --locked -p nara --test ci_policy --test artifact_package_policy --test release_verification --test reference_game_contract --test module_consumer_boundary --test-threads=1",
        ],
        "reference-game" => &[
            "uses:actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
            "run:rustup toolchain install 1.95.0 --profile minimal\nrustup default 1.95.0",
            "uses:taiki-e/install-action@43aecc8d72668fbcfe75c31400bc4f890f1c5853",
            "run:cargo check --manifest-path reference-game/Cargo.toml --locked --all-targets",
            "run:cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test authoring --test public_surface --test project_manifest_ingest --test-threads=1",
        ],
        "module-consumer" => &[
            "uses:actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
            "run:rustup toolchain install 1.95.0 --profile minimal\nrustup default 1.95.0",
            "uses:taiki-e/install-action@43aecc8d72668fbcfe75c31400bc4f890f1c5853",
            "run:cargo check --manifest-path module-consumer/Cargo.toml --locked --all-targets",
            "run:cargo nextest run --manifest-path module-consumer/Cargo.toml --locked --test-threads=1",
        ],
        _ => &[],
    }
}

fn normalize_run_script(script: &str) -> String {
    script.trim().replace("\r\n", "\n")
}

fn validate_action_pin(action: &str, job_name: &str, violations: &mut Vec<String>) {
    let pinned = action.rsplit_once('@').is_some_and(|(_, revision)| {
        revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit())
    });
    if !pinned {
        violations.push(format!(
            "job {job_name} action {action:?} must use a full commit SHA"
        ));
    }
    if !matches!(action, CHECKOUT_ACTION | INSTALL_ACTION) {
        violations.push(format!("job {job_name} uses unexpected action {action:?}"));
    }
}

fn scan_forbidden_workflow_features(node: &Yaml, path: &str, violations: &mut Vec<String>) {
    match node {
        Yaml::Hash(mapping) => {
            for (key, value) in mapping {
                let Some(key) = key.as_str() else {
                    violations.push(format!("{path} contains a non-string key"));
                    continue;
                };
                let child_path = format!("{path}.{key}");
                let normalized = key.to_ascii_lowercase();
                if matches!(normalized.as_str(), "cache" | "cache-dependency-path") {
                    violations.push(format!("{child_path} introduces a shared cache"));
                }
                if normalized == "id-token" && scalar_str(Some(value)) != Some("none") {
                    violations.push(format!("{child_path} requests OIDC authority"));
                }
                if matches!(normalized.as_str(), "if" | "continue-on-error") {
                    violations.push(format!("{child_path} uses forbidden key {key}"));
                }
                if normalized == "permissions" && path != "workflow" {
                    violations.push(format!("{child_path} overrides workflow permissions"));
                }
                scan_forbidden_workflow_features(value, &child_path, violations);
            }
        }
        Yaml::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                scan_forbidden_workflow_features(value, &format!("{path}[{index}]"), violations);
            }
        }
        Yaml::String(value) => {
            let normalized = value.to_ascii_lowercase();
            if contains_ascii_identifier(&normalized, "secrets") {
                violations.push(format!("{path} references a secret context"));
            }
            if normalized.contains("github.token") {
                violations.push(format!("{path} references github.token"));
            }
            if normalized.contains("actions_id_token_request") {
                violations.push(format!("{path} requests OIDC environment state"));
            }
            if normalized.contains("actions/cache@")
                || normalized.contains("rust-cache")
                || normalized.contains("sccache")
            {
                violations.push(format!("{path} introduces a shared cache"));
            }
            if normalized == "self-hosted" {
                violations.push(format!("{path} uses a persistent self-hosted runner"));
            }
        }
        Yaml::Alias(_) => violations.push(format!("{path} uses an uninspectable YAML alias")),
        _ => {}
    }
}

fn validate_manifest(
    name: &str,
    source: &str,
    expected_dependencies: &[&str],
    violations: &mut Vec<String>,
) {
    let manifest = match toml::from_str::<TomlValue>(source) {
        Ok(manifest) => manifest,
        Err(error) => {
            violations.push(format!("{name} manifest is invalid: {error}"));
            return;
        }
    };
    if manifest.get("patch").is_some() {
        violations.push(format!("{name} manifest contains a patch override"));
    }
    let mut dependencies = Vec::new();
    collect_dependency_entries(&manifest, &mut Vec::new(), &mut dependencies, violations);
    dependencies.sort_by(|left, right| left.0.cmp(&right.0));
    let observed = dependencies
        .iter()
        .map(|(path, _, _)| path.as_str())
        .collect::<Vec<_>>();
    if observed != expected_dependencies {
        violations.push(format!(
            "{name} has unexpected direct dependencies: {observed:?}"
        ));
    }
    for (path, dependency_name, value) in dependencies {
        let table = value.as_table();
        if table
            .and_then(|table| table.get("workspace"))
            .and_then(TomlValue::as_bool)
            == Some(true)
        {
            violations.push(format!(
                "{name} uses workspace dependency inheritance at {path}"
            ));
        }
        let package_name = table
            .and_then(|table| table.get("package"))
            .and_then(TomlValue::as_str)
            .unwrap_or(dependency_name);
        match name {
            "reference-game" if package_name != "nara" => violations.push(format!(
                "reference-game dependency {path} bypasses the root facade"
            )),
            "module-consumer"
                if package_name.starts_with("nara_")
                    && !matches!(package_name, "nara_scene" | "nara_reflect") =>
            {
                violations.push(format!(
                    "module-consumer dependency {path} uses private Nara crate {package_name}"
                ));
            }
            "module-consumer" if package_name == "nara" => violations.push(format!(
                "module-consumer dependency {path} uses the root facade"
            )),
            _ => {}
        }
    }
}

fn collect_dependency_entries<'a>(
    value: &'a TomlValue,
    path: &mut Vec<String>,
    entries: &mut Vec<(String, &'a str, &'a TomlValue)>,
    violations: &mut Vec<String>,
) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, child) in table {
        path.push(key.clone());
        if matches!(
            key.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            let Some(dependencies) = child.as_table() else {
                violations.push(format!("{} must be a dependency table", path.join(".")));
                path.pop();
                continue;
            };
            for (dependency, value) in dependencies {
                entries.push((
                    format!("{}.{}", path.join("."), dependency),
                    dependency,
                    value,
                ));
            }
        } else {
            collect_dependency_entries(child, path, entries, violations);
        }
        path.pop();
    }
}

fn field<'a>(mapping: &'a yaml_rust2::yaml::Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}

fn scalar_str(value: Option<&Yaml>) -> Option<&str> {
    value.and_then(Yaml::as_str)
}

fn string_keys(
    mapping: &yaml_rust2::yaml::Hash,
    path: &str,
    violations: &mut Vec<String>,
) -> BTreeSet<String> {
    mapping
        .keys()
        .filter_map(|key| match key.as_str() {
            Some(key) => Some(key.to_owned()),
            None => {
                violations.push(format!("{path} contains a non-string key"));
                None
            }
        })
        .collect()
}

fn validate_exact_keys(
    mapping: &yaml_rust2::yaml::Hash,
    path: &str,
    expected: &[&str],
    violations: &mut Vec<String>,
) {
    let observed = string_keys(mapping, path, violations);
    let expected = expected
        .iter()
        .copied()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if observed != expected {
        violations.push(format!(
            "{path} must contain exactly keys {expected:?}; observed {observed:?}"
        ));
    }
}

fn contains_ascii_identifier(value: &str, identifier: &str) -> bool {
    value.match_indices(identifier).any(|(start, matched)| {
        let end = start + matched.len();
        let left_is_identifier = start
            .checked_sub(1)
            .and_then(|index| value.as_bytes().get(index))
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        let right_is_identifier = value
            .as_bytes()
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        !left_is_identifier && !right_is_identifier
    })
}

fn assert_policy_accepts(fixture: &PolicyFixture) {
    let violations = fixture.violations();
    assert!(
        violations.is_empty(),
        "CI policy violations:\n{violations:#?}"
    );
}

fn assert_policy_rejects(fixture: &PolicyFixture, expected: &str) {
    let violations = fixture.violations();
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains(expected)),
        "expected violation containing {expected:?}, got:\n{violations:#?}"
    );
}

fn assert_candidate_policy_accepts(fixture: &CandidatePolicyFixture) {
    let violations = fixture.violations();
    assert!(
        violations.is_empty(),
        "candidate CI policy violations:\n{violations:#?}"
    );
}

fn assert_candidate_policy_rejects(fixture: &CandidatePolicyFixture, expected: &str) {
    let violations = fixture.violations();
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains(expected)),
        "expected candidate violation containing {expected:?}, got:\n{violations:#?}"
    );
}

fn assert_evidence_ingest_policy_accepts(fixture: &EvidenceIngestPolicyFixture) {
    let violations = fixture.violations();
    assert!(
        violations.is_empty(),
        "evidence-ingest CI policy violations:\n{violations:#?}"
    );
}

fn assert_evidence_ingest_policy_rejects(fixture: &EvidenceIngestPolicyFixture, expected: &str) {
    let violations = fixture.violations();
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains(expected)),
        "expected evidence-ingest violation containing {expected:?}, got:\n{violations:#?}"
    );
}

fn replace_first(source: &mut String, from: &str, to: &str) {
    assert!(
        source.contains(from),
        "mutation source does not contain {from:?}"
    );
    *source = source.replacen(from, to, 1);
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}
