use std::collections::BTreeSet;

use yaml_rust2::{Yaml, YamlLoader, yaml::Hash};

const WORKFLOW_PATH: &str = ".github/workflows/reference-game-release.yml";
const JOB_GUARD: &str = "${{ github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && github.run_attempt == '1' }}";
const RERUN_REJECTION_GUARD: &str = "${{ github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && github.run_attempt != '1' }}";
const GITHUB_SCRIPT: &str = "actions/github-script@d746ffe35508b1917358783b479e04febd2b8f71";
const SETUP_PYTHON: &str = "actions/setup-python@83679a892e2d95755f2dac6acb0bfd1e9ac5d548";
const DOWNLOAD_ARTIFACT: &str =
    "actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093";
const UPLOAD_ARTIFACT: &str = "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02";
const REVIEWED_RELEASE_VERIFIER_REVISION: &str = "fee339f11776eb837653bc0ca62e7db7b90eb396";
const RELEASE_VERIFIER_BLOB: &str = "a778e5eb701ae1354ed188fb7901d29d17f84e81";
const RELEASE_VERIFIER_SHA256: &str =
    "708f7995811d4a76215c58f8f6f64190f499720bc1b8c421ce90ded7b681f696";
const APPROVAL_SCHEMA_BLOB: &str = "ec84b0787434a3e7601d135835ef42d491546f38";
const APPROVAL_SCHEMA_SHA256: &str =
    "47aa27863f075aaa40a4d858ac11c5502279a80d2516e34c180f908bac752fb4";

#[derive(Clone)]
struct ReleasePolicyFixture {
    workflow: String,
}

impl ReleasePolicyFixture {
    fn committed() -> Self {
        Self {
            workflow: include_str!("../.github/workflows/reference-game-release.yml").to_owned(),
        }
    }

    fn violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        validate_release_workflow(&self.workflow, &mut violations);
        violations.sort();
        violations.dedup();
        violations
    }
}

fn repository_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn committed_release_workflow_has_only_the_reviewed_publication_authority_path() {
    assert!(
        repository_root().join(WORKFLOW_PATH).is_file(),
        "release workflow is missing"
    );
    assert_accepts(&ReleasePolicyFixture::committed());
}

#[test]
fn release_policy_rejects_untrusted_triggers_extra_write_authority_and_mutable_actions() {
    let mut trigger = ReleasePolicyFixture::committed();
    replace_once(
        &mut trigger.workflow,
        "  workflow_dispatch:\n",
        "  pull_request_target:\n",
    );
    assert_rejects(&trigger, "workflow_dispatch");

    let mut third_writer = ReleasePolicyFixture::committed();
    let public_verdict_offset = third_writer
        .workflow
        .find("  public-verdict:\n")
        .expect("public verdict job must exist");
    let mut public_verdict = third_writer.workflow.split_off(public_verdict_offset);
    replace_once(
        &mut public_verdict,
        "    permissions: {}\n",
        "    permissions:\n      contents: write\n",
    );
    third_writer.workflow.push_str(&public_verdict);
    assert_rejects(&third_writer, "only draft-upload and release-finalize");

    let mut mutable_action = ReleasePolicyFixture::committed();
    replace_once(
        &mut mutable_action.workflow,
        DOWNLOAD_ARTIFACT,
        "actions/download-artifact@v4",
    );
    assert_rejects(&mutable_action, "exact reviewed action");

    let mut oidc = ReleasePolicyFixture::committed();
    replace_once(
        &mut oidc.workflow,
        "permissions: {}\n\nconcurrency:",
        "permissions:\n  id-token: write\n\nconcurrency:",
    );
    assert_rejects(&oidc, "OIDC");
}

#[test]
fn release_policy_rejects_credential_leakage_checkout_and_candidate_execution_across_barriers() {
    let mut verifier_token = ReleasePolicyFixture::committed();
    replace_once(
        &mut verifier_token.workflow,
        "          github-token: \"\"\n          script:",
        "          github-token: \"${{ github.token }}\"\n          script:",
    );
    assert_rejects(&verifier_token, "credential-free verifier");

    let mut verifier_environment = ReleasePolicyFixture::committed();
    replace_once(
        &mut verifier_environment.workflow,
        "          TRANSPORT_ROOT: \"${{ runner.temp }}/nara-release-candidate-transport\"\n",
        "          TRANSPORT_ROOT: \"${{ runner.temp }}/nara-release-candidate-transport\"\n          NARA_LEAK: \"unexpected\"\n",
    );
    assert_rejects(
        &verifier_environment,
        "Fetch pinned verifier, schemas, and public GitHub facts.env must contain exactly keys",
    );

    let mut write_checkout = ReleasePolicyFixture::committed();
    replace_once(
        &mut write_checkout.workflow,
        "      - name: Download only manifest-bound publisher inputs\n",
        "      - name: Checkout publisher\n        uses: actions/checkout@v4\n      - name: Download only manifest-bound publisher inputs\n",
    );
    assert_rejects(
        &write_checkout,
        "write-capable job executes or checks out repository/candidate code",
    );

    let mut write_execution = ReleasePolicyFixture::committed();
    replace_once(
        &mut write_execution.workflow,
        "      - name: Upload immutable draft receipt\n",
        "      - name: Execute candidate\n        run: python -B candidate.zip\n      - name: Upload immutable draft receipt\n",
    );
    assert_rejects(&write_execution, "write-capable job executes");

    let mut public_token = ReleasePolicyFixture::committed();
    replace_once(
        &mut public_token.workflow,
        "          NARA_PUBLIC_ASSET_ROOT: \"${{ runner.temp }}/nara-release-public-assets\"\n",
        "          NARA_PUBLIC_ASSET_ROOT: \"${{ runner.temp }}/nara-release-public-assets\"\n          TOKEN: \"${{ github.token }}\"\n",
    );
    assert_rejects(&public_token, "anonymous public smoke");

    let mut unbound_archive = ReleasePolicyFixture::committed();
    replace_once(
        &mut unbound_archive.workflow,
        "              const transportArchive = archiveFromTransport(platform);\n",
        "              const transportArchive = { path: `candidate/nara-reference-game-${platform}.zip`, filename: `nara-reference-game-${platform}.zip`, size_bytes: 1, sha256: \"0\".repeat(64) };\n",
    );
    assert_rejects(&unbound_archive, "transport-bound archive identity");

    let mut indexed_draft_token = ReleasePolicyFixture::committed();
    replace_once(
        &mut indexed_draft_token.workflow,
        "          RELEASE_READ_TOKEN: \"${{ github.token }}\"\n",
        "          RELEASE_READ_TOKEN: \"${{ github.token }}\"\n          NARA_LEAK: \"${{ github['token'] }}\"\n",
    );
    assert_rejects(
        &indexed_draft_token,
        "draft smoke must not use indexed credential expressions",
    );
}

fn validate_release_workflow(source: &str, violations: &mut Vec<String>) {
    let documents = match YamlLoader::load_from_str(source) {
        Ok(documents) => documents,
        Err(error) => {
            violations.push(format!("release workflow is not valid YAML: {error}"));
            return;
        }
    };
    if documents.len() != 1 {
        violations.push("release workflow must contain exactly one YAML document".to_owned());
        return;
    }
    let workflow = &documents[0];
    let Some(root) = workflow.as_hash() else {
        violations.push("release workflow root must be a mapping".to_owned());
        return;
    };
    validate_exact_keys(
        root,
        "release workflow",
        &[
            "name",
            "run-name",
            "on",
            "permissions",
            "concurrency",
            "jobs",
        ],
        violations,
    );
    if scalar_str(field(root, "name")) != Some("Reference Game Immutable Pre-release") {
        violations.push("release workflow name is invalid".to_owned());
    }
    if scalar_str(field(root, "run-name")) != Some("Reference Game Release ${{ inputs.tag }}") {
        violations.push("release workflow run name must bind the tag".to_owned());
    }
    validate_trigger(field(root, "on"), violations);
    validate_root_permissions(field(root, "permissions"), violations);
    validate_concurrency(field(root, "concurrency"), violations);
    validate_jobs(field(root, "jobs"), violations);
    scan_global_forbidden_features(workflow, "release workflow", violations);
}

fn validate_trigger(trigger: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(trigger) = trigger.and_then(Yaml::as_hash) else {
        violations.push("release workflow must declare workflow_dispatch only".to_owned());
        return;
    };
    validate_exact_keys(
        trigger,
        "release workflow.on",
        &["workflow_dispatch"],
        violations,
    );
    let Some(dispatch) = field(trigger, "workflow_dispatch").and_then(Yaml::as_hash) else {
        violations.push("release workflow_dispatch must configure immutable locators".to_owned());
        return;
    };
    validate_exact_keys(
        dispatch,
        "release workflow.on.workflow_dispatch",
        &["inputs"],
        violations,
    );
    let Some(inputs) = field(dispatch, "inputs").and_then(Yaml::as_hash) else {
        violations.push("release workflow inputs are missing".to_owned());
        return;
    };
    let expected = [
        "approval_path",
        "approval_commit",
        "approval_blob",
        "approval_sha256",
        "tag",
        "candidate_run_id",
        "linux_artifact_id",
        "windows_artifact_id",
    ];
    if string_keys(inputs, "release workflow inputs", violations)
        != expected.iter().map(|value| (*value).to_owned()).collect()
    {
        violations.push(
            "release workflow inputs must be exact approval and candidate locators".to_owned(),
        );
    }
    for input in expected {
        let Some(definition) = field(inputs, input).and_then(Yaml::as_hash) else {
            violations.push(format!("release workflow input {input} must be a mapping"));
            continue;
        };
        validate_exact_keys(
            definition,
            &format!("release workflow input {input}"),
            &["description", "required", "type"],
            violations,
        );
        if field(definition, "required").and_then(Yaml::as_bool) != Some(true)
            || scalar_str(field(definition, "type")) != Some("string")
        {
            violations.push(format!(
                "release workflow input {input} must be required string"
            ));
        }
    }
}

fn validate_root_permissions(permissions: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(permissions) = permissions.and_then(Yaml::as_hash) else {
        violations.push("release workflow root permissions must be empty".to_owned());
        return;
    };
    if !permissions.is_empty() {
        violations.push("release workflow root permissions must be empty".to_owned());
    }
}

fn validate_concurrency(concurrency: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(concurrency) = concurrency.and_then(Yaml::as_hash) else {
        violations.push("release workflow concurrency is required".to_owned());
        return;
    };
    validate_exact_keys(
        concurrency,
        "release workflow concurrency",
        &["group", "cancel-in-progress"],
        violations,
    );
    if scalar_str(field(concurrency, "group"))
        != Some("reference-game-release-${{ github.repository }}-${{ inputs.tag }}")
    {
        violations
            .push("release workflow concurrency must be repository and tag scoped".to_owned());
    }
    if field(concurrency, "cancel-in-progress").and_then(Yaml::as_bool) != Some(false) {
        violations.push("release workflow must not cancel a tag-scoped publication".to_owned());
    }
}

fn validate_jobs(jobs: Option<&Yaml>, violations: &mut Vec<String>) {
    let Some(jobs) = jobs.and_then(Yaml::as_hash) else {
        violations.push("release workflow jobs must be a mapping".to_owned());
        return;
    };
    let expected = [
        "reject-rerun",
        "immutable-policy",
        "candidate-fetch",
        "verify",
        "draft-upload",
        "draft-smoke",
        "finalize-approval",
        "release-finalize",
        "public-smoke",
        "public-verdict",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    if string_keys(jobs, "release workflow jobs", violations) != expected {
        violations.push("release workflow must use the exact publication job graph".to_owned());
        return;
    }

    let job = |name: &str| {
        field(jobs, name)
            .and_then(Yaml::as_hash)
            .unwrap_or_else(|| panic!("expected committed job {name} to be a mapping"))
    };
    for name in [
        "immutable-policy",
        "candidate-fetch",
        "verify",
        "draft-upload",
        "draft-smoke",
        "finalize-approval",
        "release-finalize",
        "public-smoke",
        "public-verdict",
    ] {
        validate_common_job(name, job(name), violations);
        validate_action_pins(name, job(name), violations);
    }
    validate_rerun_rejection(job("reject-rerun"), violations);

    validate_job_shape(
        job("immutable-policy"),
        "immutable-policy",
        &[
            "name",
            "if",
            "runs-on",
            "timeout-minutes",
            "environment",
            "permissions",
            "outputs",
            "steps",
        ],
        violations,
    );
    validate_job_shape(
        job("candidate-fetch"),
        "candidate-fetch",
        &[
            "name",
            "if",
            "runs-on",
            "timeout-minutes",
            "permissions",
            "steps",
        ],
        violations,
    );
    validate_job_shape(
        job("verify"),
        "verify",
        &[
            "name",
            "if",
            "needs",
            "runs-on",
            "timeout-minutes",
            "permissions",
            "steps",
        ],
        violations,
    );
    validate_job_shape(
        job("draft-upload"),
        "draft-upload",
        &[
            "name",
            "if",
            "needs",
            "runs-on",
            "timeout-minutes",
            "environment",
            "permissions",
            "outputs",
            "steps",
        ],
        violations,
    );
    validate_job_shape(
        job("draft-smoke"),
        "draft-smoke",
        &[
            "name",
            "if",
            "needs",
            "runs-on",
            "timeout-minutes",
            "permissions",
            "strategy",
            "steps",
        ],
        violations,
    );
    validate_job_shape(
        job("finalize-approval"),
        "finalize-approval",
        &[
            "name",
            "if",
            "needs",
            "runs-on",
            "timeout-minutes",
            "environment",
            "permissions",
            "steps",
        ],
        violations,
    );
    validate_job_shape(
        job("release-finalize"),
        "release-finalize",
        &[
            "name",
            "if",
            "needs",
            "runs-on",
            "timeout-minutes",
            "environment",
            "permissions",
            "outputs",
            "steps",
        ],
        violations,
    );
    validate_job_shape(
        job("public-smoke"),
        "public-smoke",
        &[
            "name",
            "if",
            "needs",
            "runs-on",
            "timeout-minutes",
            "permissions",
            "strategy",
            "steps",
        ],
        violations,
    );
    validate_job_shape(
        job("public-verdict"),
        "public-verdict",
        &[
            "name",
            "if",
            "needs",
            "runs-on",
            "timeout-minutes",
            "permissions",
            "steps",
        ],
        violations,
    );

    validate_job_runner(
        job("immutable-policy"),
        "immutable-policy",
        "ubuntu-latest",
        violations,
    );
    validate_job_runner(
        job("candidate-fetch"),
        "candidate-fetch",
        "ubuntu-latest",
        violations,
    );
    validate_job_runner(job("verify"), "verify", "ubuntu-latest", violations);
    validate_job_runner(
        job("draft-upload"),
        "draft-upload",
        "ubuntu-latest",
        violations,
    );
    validate_job_runner(
        job("finalize-approval"),
        "finalize-approval",
        "ubuntu-latest",
        violations,
    );
    validate_job_runner(
        job("release-finalize"),
        "release-finalize",
        "ubuntu-latest",
        violations,
    );
    validate_job_runner(
        job("public-verdict"),
        "public-verdict",
        "ubuntu-latest",
        violations,
    );
    validate_matrix_runner(job("draft-smoke"), "draft-smoke", violations);
    validate_matrix_runner(job("public-smoke"), "public-smoke", violations);
    validate_needs(job("immutable-policy"), "immutable-policy", &[], violations);
    validate_needs(job("candidate-fetch"), "candidate-fetch", &[], violations);
    validate_needs(
        job("verify"),
        "verify",
        &["immutable-policy", "candidate-fetch"],
        violations,
    );
    validate_needs(job("draft-upload"), "draft-upload", &["verify"], violations);
    validate_needs(
        job("draft-smoke"),
        "draft-smoke",
        &["draft-upload"],
        violations,
    );
    validate_needs(
        job("finalize-approval"),
        "finalize-approval",
        &["verify", "draft-upload", "draft-smoke"],
        violations,
    );
    validate_needs(
        job("release-finalize"),
        "release-finalize",
        &["finalize-approval"],
        violations,
    );
    validate_needs(
        job("public-smoke"),
        "public-smoke",
        &["release-finalize"],
        violations,
    );
    validate_needs(
        job("public-verdict"),
        "public-verdict",
        &["verify", "release-finalize", "public-smoke"],
        violations,
    );

    validate_job_permissions(
        job("immutable-policy"),
        PermissionShape::Empty,
        "immutable-policy",
        violations,
    );
    validate_job_permissions(
        job("candidate-fetch"),
        PermissionShape::ActionsRead,
        "candidate-fetch",
        violations,
    );
    validate_job_permissions(job("verify"), PermissionShape::Empty, "verify", violations);
    validate_job_permissions(
        job("draft-upload"),
        PermissionShape::ContentsWrite,
        "draft-upload",
        violations,
    );
    validate_job_permissions(
        job("draft-smoke"),
        PermissionShape::ContentsRead,
        "draft-smoke",
        violations,
    );
    validate_job_permissions(
        job("finalize-approval"),
        PermissionShape::Empty,
        "finalize-approval",
        violations,
    );
    validate_job_permissions(
        job("release-finalize"),
        PermissionShape::ContentsWrite,
        "release-finalize",
        violations,
    );
    validate_job_permissions(
        job("public-smoke"),
        PermissionShape::Empty,
        "public-smoke",
        violations,
    );
    validate_job_permissions(
        job("public-verdict"),
        PermissionShape::Empty,
        "public-verdict",
        violations,
    );

    validate_environment(
        job("immutable-policy"),
        "reference-game-immutable-policy",
        "immutable-policy",
        violations,
    );
    validate_environment(
        job("draft-upload"),
        "reference-game-draft-upload",
        "draft-upload",
        violations,
    );
    for name in [
        "candidate-fetch",
        "verify",
        "draft-smoke",
        "public-smoke",
        "public-verdict",
    ] {
        validate_no_environment(job(name), name, violations);
    }
    validate_environment(
        job("finalize-approval"),
        "reference-game-release-finalize",
        "finalize-approval",
        violations,
    );
    validate_environment(
        job("release-finalize"),
        "reference-game-release-mutation",
        "release-finalize",
        violations,
    );

    validate_immutable_policy(job("immutable-policy"), violations);
    validate_candidate_fetch(job("candidate-fetch"), violations);
    validate_verifier(job("verify"), violations);
    validate_write_job(job("draft-upload"), "draft-upload", violations);
    validate_draft_smoke(job("draft-smoke"), violations);
    validate_finalize_approval(job("finalize-approval"), violations);
    validate_write_job(job("release-finalize"), "release-finalize", violations);
    validate_public_smoke(job("public-smoke"), violations);
    validate_public_verdict(job("public-verdict"), violations);
}

fn validate_common_job(name: &str, job: &Hash, violations: &mut Vec<String>) {
    if scalar_str(field(job, "if")) != Some(JOB_GUARD) {
        violations.push(format!(
            "{name} must require the first protected-main manual attempt"
        ));
    }
    if field(job, "timeout-minutes")
        .and_then(Yaml::as_i64)
        .is_none_or(|timeout| !(1..=45).contains(&timeout))
    {
        violations.push(format!("{name} must declare a bounded timeout"));
    }
    if !job.contains_key(&Yaml::String("runs-on".to_owned())) {
        violations.push(format!("{name} must declare a disposable runner"));
    }
    if !job.contains_key(&Yaml::String("steps".to_owned())) {
        violations.push(format!("{name} must declare its steps"));
    }
}

fn validate_rerun_rejection(job: &Hash, violations: &mut Vec<String>) {
    validate_job_shape(
        job,
        "reject-rerun",
        &[
            "name",
            "if",
            "runs-on",
            "timeout-minutes",
            "permissions",
            "steps",
        ],
        violations,
    );
    if scalar_str(field(job, "if")) != Some(RERUN_REJECTION_GUARD)
        || scalar_str(field(job, "runs-on")) != Some("ubuntu-latest")
        || field(job, "timeout-minutes").and_then(Yaml::as_i64) != Some(5)
        || step_count(job) != 1
        || !has_run_step(job)
    {
        violations
            .push("reruns must fail explicitly before any publication job can skip".to_owned());
    }
    validate_job_permissions(job, PermissionShape::Empty, "reject-rerun", violations);
}

fn validate_job_shape(job: &Hash, name: &str, expected: &[&str], violations: &mut Vec<String>) {
    validate_exact_keys(job, &format!("{name} job"), expected, violations);
}

fn validate_job_runner(job: &Hash, name: &str, expected: &str, violations: &mut Vec<String>) {
    if scalar_str(field(job, "runs-on")) != Some(expected) {
        violations.push(format!(
            "{name} must use the fixed disposable runner {expected}"
        ));
    }
}

fn validate_matrix_runner(job: &Hash, name: &str, violations: &mut Vec<String>) {
    if scalar_str(field(job, "runs-on")) != Some("${{ matrix.os }}") {
        violations.push(format!(
            "{name} must select its runner from the fixed platform matrix"
        ));
    }
    let Some(strategy) = field(job, "strategy").and_then(Yaml::as_hash) else {
        violations.push(format!("{name} must declare the fixed platform matrix"));
        return;
    };
    validate_exact_keys(
        strategy,
        &format!("{name}.strategy"),
        &["fail-fast", "matrix"],
        violations,
    );
    if field(strategy, "fail-fast").and_then(Yaml::as_bool) != Some(false) {
        violations.push(format!("{name} must retain both platform results"));
    }
    let Some(matrix) = field(strategy, "matrix").and_then(Yaml::as_hash) else {
        violations.push(format!("{name} matrix is missing"));
        return;
    };
    validate_exact_keys(
        matrix,
        &format!("{name}.strategy.matrix"),
        &["include"],
        violations,
    );
    let Some(include) = field(matrix, "include").and_then(Yaml::as_vec) else {
        violations.push(format!("{name} matrix include list is missing"));
        return;
    };
    let expected = [
        ("ubuntu-latest", "linux-x86_64"),
        ("windows-latest", "windows-x86_64"),
    ];
    if include.len() != expected.len() {
        violations.push(format!("{name} matrix must contain both fixed platforms"));
        return;
    }
    for (entry, (os, platform)) in include.iter().zip(expected) {
        let Some(entry) = entry.as_hash() else {
            violations.push(format!("{name} matrix entry must be a mapping"));
            continue;
        };
        validate_exact_keys(
            entry,
            &format!("{name} matrix entry"),
            &["os", "platform"],
            violations,
        );
        if scalar_str(field(entry, "os")) != Some(os)
            || scalar_str(field(entry, "platform")) != Some(platform)
        {
            violations.push(format!("{name} matrix platform mapping is invalid"));
        }
    }
}

fn validate_needs(job: &Hash, name: &str, expected: &[&str], violations: &mut Vec<String>) {
    let observed = match field(job, "needs") {
        None => BTreeSet::new(),
        Some(Yaml::String(single)) => BTreeSet::from([single.to_owned()]),
        Some(Yaml::Array(values)) => values
            .iter()
            .filter_map(Yaml::as_str)
            .map(str::to_owned)
            .collect(),
        Some(_) => {
            violations.push(format!("{name} must use a string list for dependencies"));
            return;
        }
    };
    let expected = expected
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if observed != expected {
        violations.push(format!(
            "{name} has an invalid publication dependency graph"
        ));
    }
}

enum PermissionShape {
    Empty,
    ActionsRead,
    ContentsRead,
    ContentsWrite,
}

fn validate_job_permissions(
    job: &Hash,
    expected: PermissionShape,
    name: &str,
    violations: &mut Vec<String>,
) {
    let Some(permissions) = field(job, "permissions").and_then(Yaml::as_hash) else {
        violations.push(format!("{name} must declare explicit permissions"));
        return;
    };
    let expected = match expected {
        PermissionShape::Empty => BTreeSet::new(),
        PermissionShape::ActionsRead => BTreeSet::from([("actions", "read")]),
        PermissionShape::ContentsRead => BTreeSet::from([("contents", "read")]),
        PermissionShape::ContentsWrite => BTreeSet::from([("contents", "write")]),
    };
    let observed = permissions
        .iter()
        .filter_map(|(key, value)| Some((key.as_str()?, value.as_str()?)))
        .collect::<BTreeSet<_>>();
    if observed != expected {
        violations.push(format!(
            "{name} has invalid permissions {observed:?}; only draft-upload and release-finalize may write contents"
        ));
    }
}

fn validate_environment(job: &Hash, expected: &str, name: &str, violations: &mut Vec<String>) {
    if scalar_str(field(job, "environment")) != Some(expected) {
        violations.push(format!("{name} must use environment {expected}"));
    }
}

fn validate_no_environment(job: &Hash, name: &str, violations: &mut Vec<String>) {
    if field(job, "environment").is_some() {
        violations.push(format!(
            "{name} must not receive an environment gate or environment secrets"
        ));
    }
}

fn validate_action_pins(name: &str, job: &Hash, violations: &mut Vec<String>) {
    let Some(steps) = field(job, "steps").and_then(Yaml::as_vec) else {
        return;
    };
    for (index, step) in steps.iter().enumerate() {
        let Some(step) = step.as_hash() else {
            violations.push(format!("{name}.steps[{index}] must be a mapping"));
            continue;
        };
        if let Some(action) = scalar_str(field(step, "uses")) {
            if !matches!(
                action,
                GITHUB_SCRIPT | SETUP_PYTHON | DOWNLOAD_ARTIFACT | UPLOAD_ARTIFACT
            ) {
                violations.push(format!(
                    "{name}.steps[{index}] must use an exact reviewed action"
                ));
            }
        }
    }
}

fn validate_immutable_policy(job: &Hash, violations: &mut Vec<String>) {
    let content = strings(job).join("\n");
    if !content.contains("secrets.NARA_RELEASE_POLICY_TOKEN")
        || !content.contains("immutable-releases")
        || !content.contains("github-token")
        || content.contains("github.token")
        || content.contains("actions/checkout@")
        || content.contains("candidate")
    {
        violations.push(
            "immutable policy must use only its policy token and immutable-release endpoint"
                .to_owned(),
        );
    }
    if step_count(job) != 1 || action_count(job, GITHUB_SCRIPT) != 1 {
        violations.push("immutable policy must have exactly one fixed read-only action".to_owned());
    }
}

fn validate_candidate_fetch(job: &Hash, violations: &mut Vec<String>) {
    let content = strings(job).join("\n");
    if step_count(job) != 4
        || action_count(job, DOWNLOAD_ARTIFACT) != 2
        || action_count(job, UPLOAD_ARTIFACT) != 2
        || has_run_step(job)
        || !content.contains("artifact-ids")
        || !content.contains("run-id")
        || count_occurrences(&content, "github.token") != 2
        || content.contains("secrets.")
        || content.contains("actions/checkout@")
    {
        violations.push(
            "candidate-fetch must use only fixed Actions read/download and staging steps"
                .to_owned(),
        );
    }
}

fn validate_verifier(job: &Hash, violations: &mut Vec<String>) {
    let content = strings(job).join("\n");
    if content.contains("github.token")
        || content.contains("secrets.")
        || content.contains("actions/checkout@")
        || content.contains("bundle-smoke")
        || content.contains(" smoke --")
        || !content.contains("REVIEWED_SOURCE_REVISION")
        || !content.contains(REVIEWED_RELEASE_VERIFIER_REVISION)
        || !content.contains(RELEASE_VERIFIER_BLOB)
        || !content.contains(RELEASE_VERIFIER_SHA256)
        || !content.contains(APPROVAL_SCHEMA_BLOB)
        || !content.contains(APPROVAL_SCHEMA_SHA256)
        || !content.contains("archiveFromTransport")
        || !content.contains("TRANSPORT_ROOT")
        || !content.contains("const publisherDefinition = await contentDigestAt(")
        || !content.contains("definition_sha256: publisherDefinition.sha256")
        || content.contains("PUBLISHER_DEFINITION_SHA256")
        || content.contains("inputs.publisher_definition_sha256")
        || !content.contains("const excludes =")
        || !content.contains("bypass_actors")
        || !content.contains("ruleset?.enforcement !== \"active\"")
        || !content.contains("tag rulesets exceed its bounded pagination budget")
        || !content
            .contains("rulesets?targets=tag&includes_parents=true&per_page=${perPage}&page=${page}")
        || !content.contains("runs?event=workflow_dispatch&per_page=100&page=")
        || content.contains("size_bytes: 0")
        || content.contains("\"0\".repeat(64)")
        || count_occurrences(&content, "env -i") < 5
        || !content.contains("bundle-verify")
        || !content.contains("build-manifest")
        || !content.contains("verify-policy")
    {
        violations.push("credential-free verifier must use pinned policy bytes and transport-bound archive identity".to_owned());
    }
    let github_script_step = step_named(
        job,
        "Fetch pinned verifier, schemas, and public GitHub facts",
    );
    if github_script_step
        .and_then(|step| field(step, "with"))
        .and_then(Yaml::as_hash)
        .and_then(|with| scalar_str(field(with, "github-token")))
        != Some("")
    {
        violations.push(
            "credential-free verifier must pass an empty github-token to github-script".to_owned(),
        );
    }
    validate_step_keys(
        job,
        "Fetch pinned verifier, schemas, and public GitHub facts",
        &["name", "uses", "env", "with"],
        violations,
    );
    validate_step_environment(
        job,
        "Fetch pinned verifier, schemas, and public GitHub facts",
        &[
            "REVIEWED_SOURCE_REVISION",
            "VERIFIER_PATH",
            "VERIFIER_BLOB",
            "VERIFIER_SHA256",
            "APPROVAL_SCHEMA_PATH",
            "APPROVAL_SCHEMA_BLOB",
            "APPROVAL_SCHEMA_SHA256",
            "TRUSTED_SCHEMA_PATH",
            "TRUSTED_SCHEMA_BLOB",
            "TRUSTED_SCHEMA_SHA256",
            "MANIFEST_SCHEMA_PATH",
            "MANIFEST_SCHEMA_BLOB",
            "MANIFEST_SCHEMA_SHA256",
            "SMOKE_HELPER_PATH",
            "SMOKE_HELPER_BLOB",
            "SMOKE_HELPER_SHA256",
            "PACKAGE_HELPER_PATH",
            "PACKAGE_HELPER_BLOB",
            "PACKAGE_HELPER_SHA256",
            "PACKAGE_LAYOUT_PATH",
            "PACKAGE_LAYOUT_BLOB",
            "PACKAGE_LAYOUT_SHA256",
            "APPROVAL_PATH",
            "APPROVAL_COMMIT",
            "APPROVAL_BLOB",
            "APPROVAL_SHA256",
            "TAG_NAME",
            "CANDIDATE_RUN_ID",
            "LINUX_ARTIFACT_ID",
            "WINDOWS_ARTIFACT_ID",
            "IMMUTABLE_RELEASES_ENABLED",
            "POLICY_ROOT",
            "TRANSPORT_ROOT",
        ],
        None,
        violations,
    );
    for (key, expected) in [
        (
            "REVIEWED_SOURCE_REVISION",
            REVIEWED_RELEASE_VERIFIER_REVISION,
        ),
        ("VERIFIER_BLOB", RELEASE_VERIFIER_BLOB),
        ("VERIFIER_SHA256", RELEASE_VERIFIER_SHA256),
        ("APPROVAL_SCHEMA_BLOB", APPROVAL_SCHEMA_BLOB),
        ("APPROVAL_SCHEMA_SHA256", APPROVAL_SCHEMA_SHA256),
    ] {
        validate_step_environment_value(
            job,
            "Fetch pinned verifier, schemas, and public GitHub facts",
            key,
            expected,
            violations,
        );
    }
    if action_count(job, GITHUB_SCRIPT) != 1
        || action_count(job, SETUP_PYTHON) != 1
        || action_count(job, DOWNLOAD_ARTIFACT) != 2
        || action_count(job, UPLOAD_ARTIFACT) != 4
    {
        violations.push("credential-free verifier has an unexpected action pipeline".to_owned());
    }
    for step in [
        "Upload bounded publisher inputs",
        "Upload bounded publication manifest",
        "Upload pinned smoke helpers",
    ] {
        validate_upload_retention(job, step, "14", violations);
    }
}

fn validate_upload_retention(job: &Hash, name: &str, expected: &str, violations: &mut Vec<String>) {
    let Some(step) = step_named(job, name) else {
        violations.push(format!("release workflow is missing upload step {name}"));
        return;
    };
    let Some(with) = field(step, "with").and_then(Yaml::as_hash) else {
        violations.push(format!(
            "release upload step {name} has no bounded artifact settings"
        ));
        return;
    };
    if scalar_str(field(with, "retention-days")) != Some(expected) {
        violations.push(format!(
            "release upload step {name} must retain inputs for {expected} days"
        ));
    }
}

fn validate_write_job(job: &Hash, name: &str, violations: &mut Vec<String>) {
    let content = strings(job).join("\n").to_ascii_lowercase();
    if content.contains("actions/checkout@")
        || content.contains("python ")
        || content.contains("cargo ")
        || content.contains("smoke_artifact.py")
        || content.contains("package.py")
        || content.contains("bundle-verify")
        || content.contains("bundle-smoke")
        || content.contains("child_process")
        || content.contains("exec(")
        || content.contains("spawn(")
    {
        violations.push(format!(
            "{name} write-capable job executes or checks out repository/candidate code"
        ));
    }
    if action_count(job, GITHUB_SCRIPT) != 1
        || action_count(job, DOWNLOAD_ARTIFACT) == 0
        || action_count(job, UPLOAD_ARTIFACT) != 1
    {
        violations.push(format!(
            "{name} must use the reviewed bounded publication action pipeline"
        ));
    }
    if name == "release-finalize"
        && (!content.contains("draft smoke receipt does not match its approved candidate")
            || !content.contains("candidate_result_sha256")
            || !content.contains("published.data.immutable !== true"))
    {
        violations.push(
            "release-finalize must recheck every draft smoke receipt against the manifest"
                .to_owned(),
        );
    }
}

fn validate_draft_smoke(job: &Hash, violations: &mut Vec<String>) {
    let content = strings(job).join("\n");
    let expressions = compact_expression_source(&content);
    if content.contains("secrets.")
        || expressions.contains("secrets.")
        || content.contains("actions/checkout@")
        || count_occurrences(&expressions, "github.token") != 1
        || !content.contains("RELEASE_READ_TOKEN")
        || !content.contains("RedirectWithoutAuthorization")
        || !content.contains("smoke_artifact.py")
        || !content.contains("env -i")
        || !content.contains("Remove-Item Env:GITHUB_TOKEN")
        || !content.contains("nara-release-draft-assets")
        || expressions.contains("github[")
        || expressions.contains("secrets[")
    {
        violations.push(
            "draft smoke must isolate candidate execution after one authenticated asset download"
                .to_owned(),
        );
    }
    if expressions.contains("github[") || expressions.contains("secrets[") {
        violations.push("draft smoke must not use indexed credential expressions".to_owned());
    }
    validate_step_keys(
        job,
        "Download exact draft asset before candidate execution",
        &["name", "env", "run"],
        violations,
    );
    validate_step_environment(
        job,
        "Download exact draft asset before candidate execution",
        &[
            "RELEASE_READ_TOKEN",
            "NARA_PLATFORM",
            "NARA_MANIFEST",
            "NARA_DRAFT_RECEIPT",
            "NARA_DRAFT_ASSET_ROOT",
        ],
        Some(("RELEASE_READ_TOKEN", "${{ github.token }}")),
        violations,
    );
    validate_step_keys(
        job,
        "Smoke Linux draft candidate without credentials",
        &["name", "if", "run"],
        violations,
    );
    validate_step_keys(
        job,
        "Smoke Windows draft candidate without credentials",
        &["name", "if", "shell", "run"],
        violations,
    );
    validate_step_without_environment(
        job,
        "Smoke Linux draft candidate without credentials",
        violations,
    );
    validate_step_without_environment(
        job,
        "Smoke Windows draft candidate without credentials",
        violations,
    );
    validate_matrix_receipt_shell(job, "Record bound draft smoke result", violations);
}

fn validate_finalize_approval(job: &Hash, violations: &mut Vec<String>) {
    let content = strings(job).join("\n").to_ascii_lowercase();
    if content.contains("github.token")
        || content.contains("secrets.")
        || content.contains("actions/checkout@")
        || content.contains("smoke_artifact.py")
        || content.contains("candidate.zip")
        || action_count(job, DOWNLOAD_ARTIFACT) != 4
        || action_count(job, UPLOAD_ARTIFACT) != 1
    {
        violations.push(
            "finalize approval must bind receipts without credentials or candidate bytes"
                .to_owned(),
        );
    }
}

fn validate_public_smoke(job: &Hash, violations: &mut Vec<String>) {
    let content = strings(job).join("\n");
    if content.contains("github.token")
        || content.contains("secrets.")
        || content.contains("Authorization")
        || content.contains("actions/checkout@")
        || !content.contains("anonymous")
        || !content.contains("browser_download_url")
        || !content.contains("smoke_artifact.py")
        || !content.contains("env -i")
        || !content.contains("Remove-Item Env:GITHUB_TOKEN")
    {
        violations
            .push("anonymous public smoke must not receive an authorization credential".to_owned());
    }
    validate_step_keys(
        job,
        "Smoke Linux public candidate without credentials",
        &["name", "if", "run"],
        violations,
    );
    validate_step_keys(
        job,
        "Smoke Windows public candidate without credentials",
        &["name", "if", "shell", "run"],
        violations,
    );
    validate_step_without_environment(
        job,
        "Smoke Linux public candidate without credentials",
        violations,
    );
    validate_step_without_environment(
        job,
        "Smoke Windows public candidate without credentials",
        violations,
    );
    validate_matrix_receipt_shell(
        job,
        "Record bound anonymous public smoke result",
        violations,
    );
}

fn validate_step_keys(job: &Hash, name: &str, expected: &[&str], violations: &mut Vec<String>) {
    let Some(step) = step_named(job, name) else {
        violations.push(format!("release workflow is missing step {name}"));
        return;
    };
    validate_exact_keys(
        step,
        &format!("release workflow step {name}"),
        expected,
        violations,
    );
}

fn validate_step_environment(
    job: &Hash,
    name: &str,
    expected: &[&str],
    required: Option<(&str, &str)>,
    violations: &mut Vec<String>,
) {
    let Some(step) = step_named(job, name) else {
        return;
    };
    let Some(environment) = field(step, "env").and_then(Yaml::as_hash) else {
        violations.push(format!(
            "release workflow step {name} must declare its bounded environment"
        ));
        return;
    };
    validate_exact_keys(
        environment,
        &format!("release workflow step {name}.env"),
        expected,
        violations,
    );
    if let Some((key, value)) = required {
        if scalar_str(field(environment, key)) != Some(value) {
            violations.push(format!(
                "release workflow step {name} must use its one scoped credential"
            ));
        }
    }
}

fn validate_step_environment_value(
    job: &Hash,
    name: &str,
    key: &str,
    expected: &str,
    violations: &mut Vec<String>,
) {
    let Some(step) = step_named(job, name) else {
        return;
    };
    let Some(environment) = field(step, "env").and_then(Yaml::as_hash) else {
        return;
    };
    if scalar_str(field(environment, key)) != Some(expected) {
        violations.push(format!(
            "release workflow step {name} must pin {key} to the reviewed value"
        ));
    }
}

fn validate_step_without_environment(job: &Hash, name: &str, violations: &mut Vec<String>) {
    let Some(step) = step_named(job, name) else {
        return;
    };
    if field(step, "env").is_some() {
        violations.push(format!(
            "release workflow candidate step {name} must not inherit custom environment values"
        ));
    }
}

fn validate_matrix_receipt_shell(job: &Hash, name: &str, violations: &mut Vec<String>) {
    validate_step_keys(job, name, &["name", "env", "shell", "run"], violations);
    let Some(step) = step_named(job, name) else {
        return;
    };
    if scalar_str(field(step, "shell")) != Some("bash") {
        violations.push(format!(
            "release workflow receipt step {name} must use Bash for its heredoc"
        ));
    }
}

fn validate_public_verdict(job: &Hash, violations: &mut Vec<String>) {
    let content = strings(job).join("\n").to_ascii_lowercase();
    if content.contains("github.token")
        || content.contains("secrets.")
        || content.contains("actions/checkout@")
        || content.contains("smoke_artifact.py")
        || !content.contains("announcement")
        || !content.contains("not-performed")
        || action_count(job, DOWNLOAD_ARTIFACT) != 4
        || action_count(job, UPLOAD_ARTIFACT) != 1
    {
        violations.push(
            "public verdict must record anonymous smoke without announcing or executing candidates"
                .to_owned(),
        );
    }
}

fn scan_global_forbidden_features(node: &Yaml, path: &str, violations: &mut Vec<String>) {
    match node {
        Yaml::Hash(mapping) => {
            for (key, value) in mapping {
                let Some(key) = key.as_str() else {
                    violations.push(format!("{path} contains a non-string key"));
                    continue;
                };
                let child_path = format!("{path}.{key}");
                if key.eq_ignore_ascii_case("id-token") && value.as_str() != Some("none") {
                    violations.push(format!("{child_path} requests OIDC"));
                }
                if key.eq_ignore_ascii_case("continue-on-error") {
                    violations.push(format!("{child_path} masks failure"));
                }
                if key.eq_ignore_ascii_case("cache")
                    || key.eq_ignore_ascii_case("cache-dependency-path")
                {
                    violations.push(format!("{child_path} introduces a shared cache"));
                }
                scan_global_forbidden_features(value, &child_path, violations);
            }
        }
        Yaml::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                scan_global_forbidden_features(value, &format!("{path}[{index}]"), violations);
            }
        }
        Yaml::String(value) => {
            let normalized = value.to_ascii_lowercase();
            if normalized.contains("pull_request")
                || normalized.contains("actions/checkout@")
                || normalized.contains("actions/cache@")
                || normalized.contains("rust-cache")
                || normalized.contains("sccache")
                || normalized == "self-hosted"
            {
                violations.push(format!(
                    "{path} contains an untrusted or persistent execution feature"
                ));
            }
        }
        Yaml::Alias(_) => violations.push(format!("{path} uses an uninspectable YAML alias")),
        _ => {}
    }
}

fn step_count(job: &Hash) -> usize {
    field(job, "steps")
        .and_then(Yaml::as_vec)
        .map_or(0, Vec::len)
}

fn action_count(job: &Hash, action: &str) -> usize {
    field(job, "steps")
        .and_then(Yaml::as_vec)
        .map(|steps| {
            steps
                .iter()
                .filter_map(Yaml::as_hash)
                .filter(|step| scalar_str(field(step, "uses")) == Some(action))
                .count()
        })
        .unwrap_or(0)
}

fn has_run_step(job: &Hash) -> bool {
    field(job, "steps")
        .and_then(Yaml::as_vec)
        .is_some_and(|steps| {
            steps
                .iter()
                .filter_map(Yaml::as_hash)
                .any(|step| step.contains_key(&Yaml::String("run".to_owned())))
        })
}

fn step_named<'a>(job: &'a Hash, expected_name: &str) -> Option<&'a Hash> {
    field(job, "steps")
        .and_then(Yaml::as_vec)
        .and_then(|steps| {
            steps
                .iter()
                .filter_map(Yaml::as_hash)
                .find(|step| scalar_str(field(step, "name")) == Some(expected_name))
        })
}

fn strings(value: &Hash) -> Vec<String> {
    let mut collected = Vec::new();
    for (key, value) in value {
        collect_strings(key, &mut collected);
        collect_strings(value, &mut collected);
    }
    collected
}

fn collect_strings(value: &Yaml, collected: &mut Vec<String>) {
    match value {
        Yaml::Hash(mapping) => {
            for (key, value) in mapping {
                collect_strings(key, collected);
                collect_strings(value, collected);
            }
        }
        Yaml::Array(values) => {
            for value in values {
                collect_strings(value, collected);
            }
        }
        Yaml::String(value) => collected.push(value.clone()),
        _ => {}
    }
}

fn count_occurrences(value: &str, needle: &str) -> usize {
    value.match_indices(needle).count()
}

fn compact_expression_source(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn field<'a>(mapping: &'a Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}

fn scalar_str(value: Option<&Yaml>) -> Option<&str> {
    value.and_then(Yaml::as_str)
}

fn string_keys(mapping: &Hash, path: &str, violations: &mut Vec<String>) -> BTreeSet<String> {
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
    mapping: &Hash,
    path: &str,
    expected: &[&str],
    violations: &mut Vec<String>,
) {
    let observed = string_keys(mapping, path, violations);
    let expected = expected
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<BTreeSet<_>>();
    if observed != expected {
        violations.push(format!(
            "{path} must contain exactly keys {expected:?}; observed {observed:?}"
        ));
    }
}

fn replace_once(source: &mut String, from: &str, to: &str) {
    assert!(
        source.contains(from),
        "fixture mutation source must exist: {from:?}"
    );
    *source = source.replacen(from, to, 1);
}

fn assert_accepts(fixture: &ReleasePolicyFixture) {
    let violations = fixture.violations();
    assert!(
        violations.is_empty(),
        "release workflow policy violations:\n{violations:#?}"
    );
}

fn assert_rejects(fixture: &ReleasePolicyFixture, expected: &str) {
    let violations = fixture.violations();
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains(expected)),
        "expected violation containing {expected:?}, got:\n{violations:#?}"
    );
}
