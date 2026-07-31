use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use toml::Value as TomlValue;
use yaml_rust2::{Yaml, YamlLoader, yaml::Hash};

const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
const CANDIDATE_WORKFLOW: &str = ".github/workflows/reference-game-candidate.yml";
const MAIN_CANDIDATE_GUARD: &str = "${{ github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && github.run_attempt == '1' }}";
const INVALID_REF_GUARD: &str =
    "${{ github.event_name == 'workflow_dispatch' && github.ref != 'refs/heads/main' }}";
const RERUN_GUARD: &str =
    "${{ github.event_name == 'workflow_dispatch' && github.run_attempt != '1' }}";
const RUNNERS: [&str; 2] = ["ubuntu-latest", "windows-latest"];
const LINUX_PACKAGES: [&str; 10] = [
    "libx11-6",
    "libx11-xcb1",
    "libxcb1",
    "libxcursor1",
    "libxi6",
    "libxkbcommon0",
    "libxkbcommon-x11-0",
    "mesa-vulkan-drivers",
    "xauth",
    "xvfb",
];

#[test]
fn hosted_ci_runs_complete_locked_suites_on_supported_platforms() {
    for lockfile in [
        "Cargo.lock",
        "reference-game/Cargo.lock",
        "module-consumer/Cargo.lock",
    ] {
        assert!(
            repository_root().join(lockfile).is_file(),
            "missing {lockfile}"
        );
    }
    assert_external_manifest_boundaries();

    let workflow = load_workflow(CI_WORKFLOW);
    let root = mapping(&workflow, "CI workflow");
    assert_read_only_permissions(root);
    assert_serial_cargo_environment(root);
    assert_ci_trigger(root);
    assert_concurrency(root, true);
    assert_workflow_safety(&workflow);

    let jobs = mapping(required_field(root, "jobs", "CI workflow"), "CI jobs");
    assert_eq!(
        string_keys(jobs),
        string_set(["root", "reference-game", "module-consumer"]),
        "CI must cover exactly the three independently locked workspaces"
    );

    let root_job = required_job(jobs, "root");
    let reference_job = required_job(jobs, "reference-game");
    let module_job = required_job(jobs, "module-consumer");
    for (name, job, timeout) in [
        ("root", root_job, 75),
        ("reference-game", reference_job, 45),
        ("module-consumer", module_job, 45),
    ] {
        assert_platform_matrix(job, name);
        assert_eq!(integer_field(job, "timeout-minutes"), timeout);
        assert!(field(job, "if").is_none(), "{name} job must not be skipped");
        assert!(
            field(job, "permissions").is_none(),
            "{name} must inherit read-only permissions"
        );
        assert_checkout(job, name, name == "root");
        assert_has_action(job, "taiki-e/install-action", name);
    }

    let root_script = step_runs(root_job);
    assert_contains_all(
        &root_script,
        &[
            "cargo fmt --all -- --check",
            "cargo check --workspace --locked --all-targets",
            "cargo check --workspace --locked --all-features --all-targets",
            "cargo nextest run --locked -p nara --test architecture_docs --test-threads=1",
            "cargo nextest run --workspace --locked -E 'not binary(architecture_docs)' --test-threads=1",
            "cargo nextest run --workspace --locked --all-features -E 'not binary(architecture_docs)' --test-threads=1",
            "--features desktop-winit,render-wgpu --example windowed_clear",
            "--features runtime-2d,desktop-winit,render-wgpu --example windowed_sprites",
            "--features runtime-ui,desktop-winit,render-wgpu --example runtime_ui_panel",
        ],
        "root CI",
    );
    for feature in [
        "runtime-core",
        "runtime-2d",
        "runtime-ui",
        "tooling",
        "asset-watch",
        "desktop-winit",
        "render-wgpu",
        "tooling-egui",
        "serde",
    ] {
        assert!(
            root_script.contains(&format!("--features {feature} --lib")),
            "root CI does not check the {feature} feature ceiling"
        );
    }

    let reference_script = step_runs(reference_job);
    assert_contains_all(
        &reference_script,
        &[
            "cargo check --manifest-path reference-game/Cargo.toml --locked --all-targets",
            "cargo check --manifest-path reference-game/Cargo.toml --locked --all-features --all-targets",
            "cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test-threads=1",
            "cargo nextest run --manifest-path reference-game/Cargo.toml --locked --all-features --test-threads=1",
        ],
        "reference-game CI",
    );

    let module_script = step_runs(module_job);
    assert_contains_all(
        &module_script,
        &[
            "cargo check --manifest-path module-consumer/Cargo.toml --locked --all-targets",
            "cargo nextest run --manifest-path module-consumer/Cargo.toml --locked --test-threads=1",
        ],
        "module-consumer CI",
    );

    let source = read(CI_WORKFLOW);
    for package in LINUX_PACKAGES {
        assert!(
            source.contains(package),
            "CI Linux profile is missing {package}"
        );
    }
    for forbidden in ["--exclude", "--no-run", "continue-on-error"] {
        assert!(
            !source.contains(forbidden),
            "CI contains forbidden shortcut {forbidden}"
        );
    }
}

#[test]
fn candidate_dispatch_builds_and_smokes_real_no_checkout_products() {
    let workflow = load_workflow(CANDIDATE_WORKFLOW);
    let root = mapping(&workflow, "candidate workflow");
    assert_read_only_permissions(root);
    assert_serial_cargo_environment(root);
    assert_manual_trigger(root);
    assert_concurrency(root, false);
    assert_workflow_safety(&workflow);

    let jobs = mapping(
        required_field(root, "jobs", "candidate workflow"),
        "candidate jobs",
    );
    assert_eq!(
        string_keys(jobs),
        string_set([
            "reject-invalid-ref",
            "reject-rerun",
            "candidate-build",
            "candidate-consumer",
        ]),
        "candidate workflow must fail invalid dispatches and retain build/consumer jobs"
    );

    assert_rejection_job(
        required_job(jobs, "reject-invalid-ref"),
        INVALID_REF_GUARD,
        "ineligible candidate source",
    );
    assert_rejection_job(
        required_job(jobs, "reject-rerun"),
        RERUN_GUARD,
        "reused workflow artifact identity",
    );

    let build = required_job(jobs, "candidate-build");
    let consumer = required_job(jobs, "candidate-consumer");
    for (name, job) in [("candidate-build", build), ("candidate-consumer", consumer)] {
        assert_eq!(
            scalar_field(job, "if"),
            MAIN_CANDIDATE_GUARD,
            "wrong {name} admission guard"
        );
        assert_eq!(integer_field(job, "timeout-minutes"), 60);
        assert_candidate_matrix(job, name);
        assert!(
            field(job, "permissions").is_none(),
            "{name} must inherit read-only permissions"
        );
    }
    assert_eq!(scalar_field(consumer, "needs"), "candidate-build");

    assert_checkout(build, "candidate-build", false);
    assert!(
        !step_uses(consumer)
            .iter()
            .any(|action| action.starts_with("actions/checkout@")),
        "the candidate consumer must not checkout the repository"
    );
    assert_eq!(
        action_names(consumer),
        string_set(["actions/download-artifact", "actions/setup-python"]),
        "the candidate consumer may only prepare Python and download the candidate bundle"
    );
    assert_has_action(build, "actions/setup-python", "candidate-build");
    assert_has_action(build, "actions/upload-artifact", "candidate-build");
    assert_has_action(consumer, "actions/setup-python", "candidate-consumer");
    assert_has_action(consumer, "actions/download-artifact", "candidate-consumer");

    let build_script = step_runs(build);
    assert_contains_all(
        &build_script,
        &[
            "cargo build --manifest-path reference-game/Cargo.toml --locked --release --bin headless",
            "cargo build --manifest-path reference-game/Cargo.toml --locked --release --features desktop --bin desktop --bin desktop_render_probe",
            "reference-game/tools/package.py create",
            "reference-game/tools/package.py bundle",
        ],
        "candidate build",
    );
    let consumer_script = step_runs(consumer);
    assert_contains_all(
        &consumer_script,
        &[
            "smoke_artifact.py\" bundle-verify",
            "smoke_artifact.py\" bundle-smoke",
            "--expected-source-revision \"${{ github.sha }}\"",
            "NARA_WGPU_FORCE_FALLBACK=1",
        ],
        "candidate consumer",
    );
    let normalized_consumer_script = consumer_script.to_ascii_lowercase();
    for forbidden in [
        "git clone",
        "git checkout",
        "git worktree",
        "cargo ",
        "github.com",
        "gitlab.com",
        "curl ",
        "wget ",
        "invoke-webrequest",
    ] {
        assert!(
            !normalized_consumer_script.contains(forbidden),
            "candidate consumer must not acquire or rebuild source through `{forbidden}`"
        );
    }

    let source = read(CANDIDATE_WORKFLOW);
    for package in LINUX_PACKAGES {
        assert!(
            source.contains(package),
            "candidate Linux profile is missing {package}"
        );
    }
    for identity_part in ["${{ github.run_id }}", "${{ github.run_attempt }}"] {
        assert!(
            source.matches(identity_part).count() >= 2,
            "candidate artifact identity omits {identity_part}"
        );
    }
}

fn assert_external_manifest_boundaries() {
    let reference = parse_manifest("reference-game/Cargo.toml");
    assert!(
        reference.get("patch").is_none(),
        "reference-game must not patch dependencies"
    );
    assert_eq!(
        dependency_keys(&reference),
        owned_string_set(["dependencies.nara"]),
        "reference-game must consume only the root facade"
    );
    assert_no_workspace_dependencies(&reference, "reference-game");

    let module = parse_manifest("module-consumer/Cargo.toml");
    assert!(
        module.get("patch").is_none(),
        "module-consumer must not patch dependencies"
    );
    assert_eq!(
        dependency_keys(&module),
        owned_string_set([
            "dependencies.nara_reflect",
            "dependencies.nara_scene",
            "dev-dependencies.bevy_ecs",
        ]),
        "module-consumer must remain a direct-domain fixture"
    );
    assert_no_workspace_dependencies(&module, "module-consumer");
}

fn parse_manifest(path: &str) -> TomlValue {
    toml::from_str(&read(path)).unwrap_or_else(|error| panic!("invalid {path}: {error}"))
}

fn dependency_keys(manifest: &TomlValue) -> BTreeSet<String> {
    ["dependencies", "dev-dependencies", "build-dependencies"]
        .into_iter()
        .flat_map(|section| {
            manifest
                .get(section)
                .and_then(TomlValue::as_table)
                .into_iter()
                .flat_map(move |dependencies| {
                    dependencies
                        .keys()
                        .map(move |dependency| format!("{section}.{dependency}"))
                })
        })
        .collect()
}

fn assert_no_workspace_dependencies(manifest: &TomlValue, name: &str) {
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(dependencies) = manifest.get(section).and_then(TomlValue::as_table) else {
            continue;
        };
        for (dependency, value) in dependencies {
            assert_ne!(
                value
                    .as_table()
                    .and_then(|table| table.get("workspace"))
                    .and_then(TomlValue::as_bool),
                Some(true),
                "{name} must not inherit {section}.{dependency} from the root workspace"
            );
        }
    }
}

fn assert_ci_trigger(root: &Hash) {
    let trigger = mapping(required_field(root, "on", "CI workflow"), "CI trigger");
    assert_eq!(string_keys(trigger), string_set(["pull_request", "push"]));
    let push = mapping(
        required_field(trigger, "push", "CI trigger"),
        "push trigger",
    );
    assert_eq!(
        string_sequence(required_field(push, "branches", "push trigger")),
        string_set(["main"])
    );
}

fn assert_manual_trigger(root: &Hash) {
    let trigger = mapping(
        required_field(root, "on", "candidate workflow"),
        "candidate trigger",
    );
    assert_eq!(string_keys(trigger), string_set(["workflow_dispatch"]));
    assert!(matches!(
        field(trigger, "workflow_dispatch"),
        Some(Yaml::Null)
    ));
}

fn assert_read_only_permissions(root: &Hash) {
    let permissions = mapping(
        required_field(root, "permissions", "workflow"),
        "permissions",
    );
    assert_eq!(string_keys(permissions), string_set(["contents"]));
    assert_eq!(scalar_field(permissions, "contents"), "read");
}

fn assert_serial_cargo_environment(root: &Hash) {
    let environment = mapping(required_field(root, "env", "workflow"), "workflow env");
    assert_eq!(scalar_field(environment, "CARGO_BUILD_JOBS"), "1");
    assert_eq!(scalar_field(environment, "CARGO_INCREMENTAL"), "0");
}

fn assert_concurrency(root: &Hash, cancel_in_progress: bool) {
    let concurrency = mapping(
        required_field(root, "concurrency", "workflow"),
        "concurrency",
    );
    let group = scalar_field(concurrency, "group");
    assert!(group.contains("${{ github.workflow }}") && group.contains("github.ref"));
    assert_eq!(
        field(concurrency, "cancel-in-progress").and_then(Yaml::as_bool),
        Some(cancel_in_progress)
    );
}

fn assert_platform_matrix(job: &Hash, name: &str) {
    let strategy = mapping(required_field(job, "strategy", name), "job strategy");
    assert_eq!(
        field(strategy, "fail-fast").and_then(Yaml::as_bool),
        Some(false)
    );
    let matrix = mapping(required_field(strategy, "matrix", name), "job matrix");
    assert_eq!(
        string_sequence(required_field(matrix, "os", name)),
        string_set(RUNNERS)
    );
}

fn assert_candidate_matrix(job: &Hash, name: &str) {
    let strategy = mapping(required_field(job, "strategy", name), "candidate strategy");
    assert_eq!(
        field(strategy, "fail-fast").and_then(Yaml::as_bool),
        Some(false)
    );
    let matrix = mapping(required_field(strategy, "matrix", name), "candidate matrix");
    let include = sequence(
        required_field(matrix, "include", name),
        "candidate matrix include",
    );
    let observed = include
        .iter()
        .map(|entry| {
            let entry = mapping(entry, "candidate matrix row");
            (
                scalar_field(entry, "os").to_owned(),
                scalar_field(entry, "platform").to_owned(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        observed,
        [
            ("ubuntu-latest".to_owned(), "linux-x86_64".to_owned()),
            ("windows-latest".to_owned(), "windows-x86_64".to_owned()),
        ]
        .into_iter()
        .collect(),
        "{name} must cover the supported candidate platforms"
    );
}

fn assert_rejection_job(job: &Hash, guard: &str, reason: &str) {
    assert_eq!(scalar_field(job, "if"), guard);
    assert_eq!(scalar_field(job, "runs-on"), "ubuntu-latest");
    assert_eq!(integer_field(job, "timeout-minutes"), 5);
    assert!(
        mapping(
            required_field(job, "permissions", "rejection job"),
            "rejection permissions"
        )
        .is_empty()
    );
    let steps = job_steps(job);
    assert_eq!(steps.len(), 1);
    let step = mapping(&steps[0], "rejection step");
    assert!(scalar_field(step, "name").contains(reason));
    assert_eq!(scalar_field(step, "run"), "exit 1");
}

fn assert_checkout(job: &Hash, name: &str, full_history: bool) {
    let checkout = job_steps(job)
        .iter()
        .map(|step| mapping(step, "workflow step"))
        .find(|step| {
            field(step, "uses")
                .and_then(Yaml::as_str)
                .is_some_and(|action| action.starts_with("actions/checkout@"))
        })
        .unwrap_or_else(|| panic!("{name} lacks the pinned checkout action"));
    let options = mapping(required_field(checkout, "with", name), "checkout options");
    assert_eq!(
        field(options, "persist-credentials").and_then(Yaml::as_bool),
        Some(false)
    );
    if full_history {
        assert_eq!(integer_field(options, "fetch-depth"), 0);
    }
}

fn assert_has_action(job: &Hash, expected: &str, owner: &str) {
    assert!(
        step_uses(job)
            .iter()
            .any(|action| action.starts_with(&format!("{expected}@"))),
        "{owner} must use {expected}"
    );
}

fn assert_workflow_safety(workflow: &Yaml) {
    scan_safety(workflow, "workflow");
    let root = mapping(workflow, "workflow");
    let jobs = mapping(required_field(root, "jobs", "workflow"), "jobs");
    for (name, job) in jobs {
        let name = name.as_str().expect("job names must be strings");
        let job = mapping(job, name);
        if let Some(permissions) = field(job, "permissions") {
            assert!(
                mapping(permissions, "job permissions").is_empty(),
                "{name} adds job authority"
            );
        }
        for action in step_uses(job) {
            let Some((_, revision)) = action.rsplit_once('@') else {
                panic!("{name} action is not revision pinned: {action}");
            };
            assert!(
                revision.len() == 40
                    && revision
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "{name} action is not pinned to a lowercase commit: {action}"
            );
        }
    }
}

fn scan_safety(node: &Yaml, path: &str) {
    match node {
        Yaml::String(value) => {
            let normalized = value.to_ascii_lowercase();
            for forbidden in [
                "${{ secrets.",
                "${{ github.token",
                "actions_id_token_request",
                "pull_request_target",
                "workflow_run",
                "actions/cache@",
                "rust-cache",
                "sccache",
                "self-hosted",
            ] {
                assert!(
                    !normalized.contains(forbidden),
                    "{path} contains forbidden {forbidden}"
                );
            }
        }
        Yaml::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                scan_safety(value, &format!("{path}[{index}]"));
            }
        }
        Yaml::Hash(values) => {
            for (key, value) in values {
                let key = key.as_str().expect("workflow keys must be strings");
                assert_ne!(key, "continue-on-error", "{path} masks a failure");
                scan_safety(value, &format!("{path}.{key}"));
            }
        }
        Yaml::Alias(_) => panic!("{path} uses an uninspectable YAML alias"),
        _ => {}
    }
}

fn step_runs(job: &Hash) -> String {
    job_steps(job)
        .iter()
        .filter_map(|step| mapping(step, "workflow step").get(&Yaml::String("run".to_owned())))
        .filter_map(Yaml::as_str)
        .collect::<Vec<_>>()
        .join("\n")
}

fn step_uses(job: &Hash) -> Vec<&str> {
    job_steps(job)
        .iter()
        .filter_map(|step| mapping(step, "workflow step").get(&Yaml::String("uses".to_owned())))
        .filter_map(Yaml::as_str)
        .collect()
}

fn action_names(job: &Hash) -> BTreeSet<&str> {
    step_uses(job)
        .into_iter()
        .map(|action| {
            action
                .split_once('@')
                .map(|(name, _)| name)
                .expect("workflow actions must be revision pinned")
        })
        .collect()
}

fn job_steps(job: &Hash) -> &[Yaml] {
    sequence(required_field(job, "steps", "job"), "job steps")
}

fn assert_contains_all(source: &str, required: &[&str], owner: &str) {
    for required in required {
        assert!(source.contains(required), "{owner} is missing `{required}`");
    }
}

fn load_workflow(path: &str) -> Yaml {
    let source = read(path);
    let mut documents = YamlLoader::load_from_str(&source)
        .unwrap_or_else(|error| panic!("invalid {path}: {error}"));
    assert_eq!(documents.len(), 1, "{path} must contain one YAML document");
    documents.remove(0)
}

fn required_job<'a>(jobs: &'a Hash, name: &str) -> &'a Hash {
    mapping(required_field(jobs, name, "jobs"), name)
}

fn required_field<'a>(mapping: &'a Hash, key: &str, owner: &str) -> &'a Yaml {
    field(mapping, key).unwrap_or_else(|| panic!("{owner} lacks {key}"))
}

fn field<'a>(mapping: &'a Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}

fn mapping<'a>(value: &'a Yaml, owner: &str) -> &'a Hash {
    value
        .as_hash()
        .unwrap_or_else(|| panic!("{owner} must be a mapping"))
}

fn sequence<'a>(value: &'a Yaml, owner: &str) -> &'a [Yaml] {
    value
        .as_vec()
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("{owner} must be a sequence"))
}

fn scalar_field<'a>(mapping: &'a Hash, key: &str) -> &'a str {
    required_field(mapping, key, "mapping")
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be a string"))
}

fn integer_field(mapping: &Hash, key: &str) -> i64 {
    required_field(mapping, key, "mapping")
        .as_i64()
        .unwrap_or_else(|| panic!("{key} must be an integer"))
}

fn string_keys(mapping: &Hash) -> BTreeSet<&str> {
    mapping
        .keys()
        .map(|key| key.as_str().expect("mapping keys must be strings"))
        .collect()
}

fn string_sequence(value: &Yaml) -> BTreeSet<&str> {
    sequence(value, "string sequence")
        .iter()
        .map(|value| value.as_str().expect("sequence values must be strings"))
        .collect()
}

fn string_set<const N: usize>(values: [&'static str; N]) -> BTreeSet<&'static str> {
    values.into_iter().collect()
}

fn owned_string_set<const N: usize>(values: [&str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_owned).collect()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repository_root().join(path))
        .unwrap_or_else(|error| panic!("cannot read {path}: {error}"))
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}
