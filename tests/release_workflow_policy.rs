use std::collections::BTreeSet;

use yaml_rust2::{yaml::Hash, Yaml, YamlLoader};

const WORKFLOW_PATH: &str = ".github/workflows/reference-game-release.yml";
const JOB_GUARD: &str =
    "${{ github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && github.run_attempt == '1' }}";
const GITHUB_SCRIPT: &str = "actions/github-script@d746ffe35508b1917358783b479e04febd2b8f71";
const SETUP_PYTHON: &str = "actions/setup-python@83679a892e2d95755f2dac6acb0bfd1e9ac5d548";
const DOWNLOAD_ARTIFACT: &str =
    "actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093";
const UPLOAD_ARTIFACT: &str = "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02";
const REVIEWED_RELEASE_VERIFIER_REVISION: &str = "0d70fcf0b86e241bebb54c22a55350d5bf824680";
const RELEASE_VERIFIER_BLOB: &str = "3ef54305cb68f5b4076a37366dbd1a857d3d02e3";
const RELEASE_VERIFIER_SHA256: &str =
    "7421e9e0449802b580465f36dd9c002d776ec0adc10006154080db532b7f45bd";

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
        "publisher_definition_sha256",
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
        violations.push("release workflow must use the exact nine-stage job graph".to_owned());
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
        || !content.contains("archiveFromTransport")
        || !content.contains("TRANSPORT_ROOT")
        || !content.contains("PUBLISHER_DEFINITION_SHA256")
        || !content.contains("publisher workflow does not match the reviewed definition digest")
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
    if action_count(job, GITHUB_SCRIPT) != 1
        || action_count(job, SETUP_PYTHON) != 1
        || action_count(job, DOWNLOAD_ARTIFACT) != 2
        || action_count(job, UPLOAD_ARTIFACT) != 4
    {
        violations.push("credential-free verifier has an unexpected action pipeline".to_owned());
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
            || !content.contains("candidate_result_sha256"))
    {
        violations.push(
            "release-finalize must recheck every draft smoke receipt against the manifest"
                .to_owned(),
        );
    }
}

fn validate_draft_smoke(job: &Hash, violations: &mut Vec<String>) {
    let content = strings(job).join("\n");
    if content.contains("secrets.")
        || content.contains("actions/checkout@")
        || count_occurrences(&content, "github.token") != 1
        || !content.contains("RELEASE_READ_TOKEN")
        || !content.contains("RedirectWithoutAuthorization")
        || !content.contains("smoke_artifact.py")
        || !content.contains("env -i")
        || !content.contains("Remove-Item Env:GITHUB_TOKEN")
        || !content.contains("nara-release-draft-assets")
    {
        violations.push(
            "draft smoke must isolate candidate execution after one authenticated asset download"
                .to_owned(),
        );
    }
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
