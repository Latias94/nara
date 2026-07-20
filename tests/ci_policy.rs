use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use toml::Value as TomlValue;
use yaml_rust2::{Yaml, YamlLoader};

const WORKFLOW_PATH: &str = ".github/workflows/ci.yml";
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
const CONCURRENCY_GROUP: &str =
    "ci-${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}";

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
            "run:cargo nextest run --locked -p nara --test ci_policy --test reference_game_contract --test module_consumer_boundary --test-threads=1",
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
