use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use nara::{
    fs::{ContentDigest, RelativeComponent},
    project_workflow::{
        ProjectExportBinaryTarget, ProjectExportMemberKind, ProjectExportMemberReceipt,
        ProjectExportRequest, StartupContentClosureReceipt, StartupContentMemberKind,
        prepare_project_export,
    },
};
use serde_json::{Value, json};

use super::{
    acceptance::{self, ProductProcessObservationReceipt},
    cargo,
};
use crate::{args::CheckArguments, check, host_fs};

const PROJECT_SLUG: &str = "fresh-project-trajectory";
const REPLACEMENT_SCENE: &str = "repaired.scene.json";
const EDITED_CLEAR_COLOR_R: f64 = 0.14;
static NEXT_TRAJECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
#[ignore = "runs nested Cargo builds and a real headless/Winit/WGPU product trajectory"]
fn fresh_project_export_repair_is_one_linked_product_trajectory() {
    let started = Instant::now();
    let root = TrajectoryRoot::new();
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_parent = root.path().join("projects");
    let export_parent = root.path().join("exports");
    let unrelated_cwd = root.path().join("unrelated-cwd");
    let cargo_target = root.shared_cargo_target();
    fs::create_dir_all(&project_parent).unwrap();
    fs::create_dir_all(&export_parent).unwrap();
    fs::create_dir_all(&unrelated_cwd).unwrap();
    fs::create_dir_all(&cargo_target).unwrap();
    let mut public_commands = PublicCommandRehearsal::new(&checkout, &cargo_target);

    let create_started = Instant::now();
    public_commands.expect_success(
        &project_parent,
        [
            OsString::from("new"),
            OsString::from("desktop-2d/v1"),
            OsString::from(PROJECT_SLUG),
            OsString::from("--parent"),
            OsString::from("."),
            OsString::from("--local-checkout"),
            checkout.as_os_str().to_owned(),
        ],
    );
    let create_elapsed_ms = elapsed_ms(create_started);
    let project = project_parent.join(PROJECT_SLUG);
    let created = check::execute(CheckArguments {
        project: project.clone(),
        profile: None,
    })
    .unwrap();
    let creation_evidence = json!({
        "starter": "desktop-2d/v1",
        "dependency_kind": "LocalCheckout",
        "author_guide": digest_value(ContentDigest::of_bytes(
            &fs::read(project.join("README.md")).unwrap(),
        )),
        "manifest": digest_value(created.manifest_digest()),
        "content": digest_value(created.content_digest()),
    });
    let created_content_digest = created.content_digest();
    drop(created);

    let edited_scene_digest = edit_startup_scene(&project);
    public_commands.expect_success(&project, ["check", "."].map(OsString::from));
    let checked_before = check_products(&project);
    assert_ne!(
        checked_before.headless.content_digest(),
        created_content_digest
    );
    generate_lockfile(&project, &unrelated_cwd);

    let first_run_started = Instant::now();
    let source_before = run_source_products(
        &mut public_commands,
        "before-source",
        &project,
        &unrelated_cwd,
        &cargo_target,
        &checked_before,
    );
    let first_run_elapsed_ms = elapsed_ms(first_run_started);

    let first_export_started = Instant::now();
    let before_export = export_project(
        &mut public_commands,
        &project,
        &export_parent,
        &unrelated_cwd,
        &cargo_target,
        "candidate-before-repair",
        checked_before,
    );
    let first_export_elapsed_ms = elapsed_ms(first_export_started);

    let original_scene = project.join("scenes/startup.scene.json");
    let replacement_scene = project.join(format!("scenes/{REPLACEMENT_SCENE}"));
    fs::rename(&original_scene, &replacement_scene).unwrap();
    assert_eq!(
        ContentDigest::of_bytes(&fs::read(&replacement_scene).unwrap()),
        edited_scene_digest
    );

    let repair_started = Instant::now();
    let missing_output =
        public_commands.expect_failure(&project, ["check", "."].map(OsString::from));
    let missing_stderr = String::from_utf8_lossy(&missing_output.stderr);
    assert!(missing_stderr.contains("startup Scene `startup.scene.json` is missing"));
    assert!(missing_stderr.contains("nara repair startup-scene"));
    let missing = check::execute(CheckArguments {
        project: project.clone(),
        profile: None,
    })
    .unwrap_err();
    let issue = missing
        .startup_scene_issue()
        .expect("moving the selected startup Scene must produce a repairable issue");
    assert_eq!(issue.selected_scene().as_str(), "startup.scene.json");
    assert_eq!(issue.scenes_path().as_str(), "scenes");
    let issue_manifest_digest = issue.manifest_digest();
    let issue_evidence = json!({
        "manifest": digest_value(issue_manifest_digest),
        "scenes_root": issue.scenes_path().as_str(),
        "selected_scene": issue.selected_scene().as_str(),
    });
    drop(missing);

    public_commands.expect_success(
        &project,
        [
            OsString::from("repair"),
            OsString::from("startup-scene"),
            OsString::from("."),
            OsString::from("--replacement"),
            OsString::from(REPLACEMENT_SCENE),
        ],
    );
    let repair_elapsed_ms = elapsed_ms(repair_started);

    before_export.verify_unchanged();
    public_commands.expect_success(&project, ["check", "."].map(OsString::from));
    let checked_after = check_products(&project);
    let after_manifest_digest = checked_after.headless.manifest_digest();
    let after_content_digest = checked_after.headless.content_digest();
    assert_ne!(after_manifest_digest, issue_manifest_digest);
    assert_eq!(
        checked_after
            .headless
            .members()
            .iter()
            .find(|member| {
                member.kind() == StartupContentMemberKind::Scene
                    && member.path().as_str() == format!("scenes/{REPLACEMENT_SCENE}")
            })
            .expect("the repaired closure contains the selected replacement Scene")
            .digest(),
        edited_scene_digest,
    );
    let repair_evidence = json!({
        "previous_manifest": digest_value(issue_manifest_digest),
        "manifest": digest_value(after_manifest_digest),
        "replacement": REPLACEMENT_SCENE,
        "replacement_scene": digest_value(edited_scene_digest),
        "content": digest_value(after_content_digest),
    });
    let source_after = run_source_products(
        &mut public_commands,
        "after-source",
        &project,
        &unrelated_cwd,
        &cargo_target,
        &checked_after,
    );
    let after_export = export_project(
        &mut public_commands,
        &project,
        &export_parent,
        &unrelated_cwd,
        &cargo_target,
        "candidate-after-repair",
        checked_after,
    );

    let record = json!({
        "version": 1,
        "trajectory": "fresh-project-source-carrying-export-repair",
        "project": PROJECT_SLUG,
        "create": creation_evidence,
        "ordinary_edit": {
            "path": "scenes/startup.scene.json",
            "clear_color_r": EDITED_CLEAR_COLOR_R,
            "digest": digest_value(edited_scene_digest),
        },
        "before_repair": {
            "check": checked_value(source_before.manifest_digest(), source_before.content_digest()),
            "source_products": source_before.value(),
            "export": before_export.value(),
        },
        "issue": issue_evidence,
        "repair": repair_evidence,
        "after_repair": {
            "check": checked_value(after_manifest_digest, after_content_digest),
            "source_products": source_after.value(),
            "export": after_export.value(),
        },
        "observations": {
            "public_command_count": public_commands.count(),
            "create_ms": create_elapsed_ms,
            "first_run_ms": first_run_elapsed_ms,
            "first_export_ms": first_export_elapsed_ms,
            "repair_ms": repair_elapsed_ms,
            "total_ms": elapsed_ms(started),
        },
    });
    println!("NARA_FPER_TRAJECTORY={record}");
}

struct CheckedProducts {
    headless: StartupContentClosureReceipt,
    desktop: StartupContentClosureReceipt,
}

fn check_products(project: &Path) -> CheckedProducts {
    let headless = check::execute(CheckArguments {
        project: project.to_path_buf(),
        profile: None,
    })
    .unwrap();
    let desktop = check::execute(CheckArguments {
        project: project.to_path_buf(),
        profile: Some("desktop".to_owned()),
    })
    .unwrap();
    assert_eq!(headless.manifest_digest(), desktop.manifest_digest());
    assert_eq!(headless.content_digest(), desktop.content_digest());
    assert_ne!(headless.settings_lineage(), desktop.settings_lineage());
    CheckedProducts { headless, desktop }
}

struct PublicCommandRehearsal {
    executable: PathBuf,
    cargo_target: PathBuf,
    count: u64,
}

impl PublicCommandRehearsal {
    fn new(checkout: &Path, cargo_target: &Path) -> Self {
        Self {
            executable: build_nara_cli(checkout),
            cargo_target: cargo_target.to_path_buf(),
            count: 0,
        }
    }

    fn expect_success<I, S>(&mut self, working_directory: &Path, arguments: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.run(working_directory, arguments, true)
    }

    fn expect_failure<I, S>(&mut self, working_directory: &Path, arguments: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.run(working_directory, arguments, false)
    }

    fn expect_export_success<I, S>(
        &mut self,
        working_directory: &Path,
        observation_directory: &Path,
        arguments: I,
    ) -> Value
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let sequence = self.count.checked_add(1).expect("command count is bounded");
        let attempt = format!("fper-export-{}-{sequence}", std::process::id());
        let observation = observation_directory.join(format!("{attempt}.json"));
        let mut command = Command::new(&self.executable);
        command
            .current_dir(working_directory)
            .args(arguments)
            .env(super::COMMAND_OBSERVATION_ATTEMPT_ENV, &attempt)
            .env(super::COMMAND_OBSERVATION_FILE_ENV, &observation);
        self.finish(command, true);
        let bytes = fs::read(&observation).unwrap();
        fs::remove_file(&observation).unwrap();
        let value = serde_json::from_slice::<Value>(&bytes).unwrap();
        assert_eq!(
            value.get("attempt").and_then(Value::as_str),
            Some(attempt.as_str())
        );
        value
    }

    fn record_product_command(&mut self) {
        self.count = self.count.checked_add(1).expect("command count is bounded");
    }

    const fn count(&self) -> u64 {
        self.count
    }

    fn run<I, S>(&mut self, working_directory: &Path, arguments: I, expect_success: bool) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.executable);
        command.current_dir(working_directory).args(arguments);
        self.finish(command, expect_success)
    }

    fn finish(&mut self, mut command: Command, expect_success: bool) -> Output {
        command.env("CARGO_TARGET_DIR", &self.cargo_target);
        let output = command.output().unwrap();
        self.count = self.count.checked_add(1).expect("command count is bounded");
        assert_eq!(
            output.status.success(),
            expect_success,
            "public nara command had an unexpected status\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        output
    }
}

fn build_nara_cli(checkout: &Path) -> PathBuf {
    let target = checkout.join("target");
    let status = Command::new(env!("CARGO"))
        .current_dir(checkout)
        .env("CARGO_TARGET_DIR", &target)
        .args([
            "build",
            "--locked",
            "--no-default-features",
            "--features",
            "project-workflow-cli",
            "--bin",
            "nara",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "failed to build the public nara CLI");
    let executable = target
        .join("debug")
        .join(format!("nara{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.is_file(), "the public nara CLI was not emitted");
    executable
}

struct SourceProductEvidence {
    build: Value,
    products: [ProductProcessObservationReceipt; 2],
}

impl SourceProductEvidence {
    fn value(&self) -> Value {
        json!({
            "build": self.build.clone(),
            "products": self.products.iter().map(product_value).collect::<Vec<_>>(),
        })
    }

    fn manifest_digest(&self) -> ContentDigest {
        self.products[0].manifest_digest()
    }

    fn content_digest(&self) -> ContentDigest {
        self.products[0].startup_content_digest()
    }
}

fn run_source_products(
    public_commands: &mut PublicCommandRehearsal,
    attempt_prefix: &str,
    project: &Path,
    working_directory: &Path,
    target_directory: &Path,
    checked: &CheckedProducts,
) -> SourceProductEvidence {
    let headless_path = prepare_source_executable(
        project,
        target_directory,
        ProjectExportBinaryTarget::Headless,
    );
    let headless_digest = ContentDigest::of_bytes(&fs::read(&headless_path).unwrap());
    public_commands.record_product_command();
    let headless = acceptance::observe_prepared_product_command(
        attempt_prefix,
        acceptance::ExpectedProductObservation::new(
            ProjectExportBinaryTarget::Headless,
            &headless_path,
            headless_digest,
            checked.headless.manifest_digest(),
            checked.headless.content_digest(),
        ),
        working_directory,
        source_product_command(
            project,
            target_directory,
            ProjectExportBinaryTarget::Headless,
        ),
    )
    .unwrap();

    let desktop_path = prepare_source_executable(
        project,
        target_directory,
        ProjectExportBinaryTarget::Desktop,
    );
    let desktop_digest = ContentDigest::of_bytes(&fs::read(&desktop_path).unwrap());
    public_commands.record_product_command();
    let desktop = acceptance::observe_prepared_product_command(
        attempt_prefix,
        acceptance::ExpectedProductObservation::new(
            ProjectExportBinaryTarget::Desktop,
            &desktop_path,
            desktop_digest,
            checked.desktop.manifest_digest(),
            checked.desktop.content_digest(),
        ),
        working_directory,
        source_product_command(
            project,
            target_directory,
            ProjectExportBinaryTarget::Desktop,
        ),
    )
    .unwrap();
    let build_value = json!({
        "profile": "dev",
        "manifest": digest_value(ContentDigest::of_bytes(&fs::read(project.join("Cargo.toml")).unwrap())),
        "lock": digest_value(ContentDigest::of_bytes(&fs::read(project.join("Cargo.lock")).unwrap())),
        "executables": [
            { "target": "headless", "digest": digest_value(headless_digest) },
            { "target": "desktop", "digest": digest_value(desktop_digest) },
        ],
    });
    SourceProductEvidence {
        build: build_value,
        products: [headless, desktop],
    }
}

fn prepare_source_executable(
    project: &Path,
    target_directory: &Path,
    target: ProjectExportBinaryTarget,
) -> PathBuf {
    let mut command = Command::new(env!("CARGO"));
    command
        .arg("build")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(project.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target_directory)
        .arg("--no-default-features");
    if target == ProjectExportBinaryTarget::Desktop {
        command.args(["--features", "desktop"]);
    }
    let status = command
        .arg("--bin")
        .arg(target.cargo_target_name())
        .status()
        .unwrap();
    assert!(status.success(), "failed to prepare the source product");
    let executable = target_directory.join("debug").join(format!(
        "{}{}",
        target.cargo_target_name(),
        std::env::consts::EXE_SUFFIX,
    ));
    assert!(executable.is_file(), "the source product was not emitted");
    executable
}

fn source_product_command(
    project: &Path,
    target_directory: &Path,
    target: ProjectExportBinaryTarget,
) -> Command {
    let mut command = Command::new(env!("CARGO"));
    command
        .arg("run")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(project.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target_directory)
        .arg("--no-default-features");
    if target == ProjectExportBinaryTarget::Desktop {
        command.args(["--features", "desktop"]);
    }
    command.arg("--bin").arg(target.cargo_target_name());
    if target == ProjectExportBinaryTarget::Desktop {
        command.args(["--", "--verify-first-present"]);
    }
    command
}

struct ExportPlanEvidence {
    startup_content_digest: ContentDigest,
    build_input_digest: ContentDigest,
    member_plan_digest: ContentDigest,
    members: Box<[ProjectExportMemberReceipt]>,
}

struct AcceptedExport {
    root: PathBuf,
    destination: String,
    plan: ExportPlanEvidence,
    products: Value,
}

impl AcceptedExport {
    fn verify_unchanged(&self) {
        for member in &self.plan.members {
            host_fs::readable_file(&self.root.join(member.path().as_str()))
                .unwrap()
                .verify_digest(member.digest(), member.digest().length())
                .unwrap();
        }
    }

    fn value(&self) -> Value {
        json!({
            "destination": self.destination.as_str(),
            "startup_content": digest_value(self.plan.startup_content_digest),
            "build_input": digest_value(self.plan.build_input_digest),
            "member_plan": digest_value(self.plan.member_plan_digest),
            "products": self.products.clone(),
        })
    }
}

fn export_project(
    public_commands: &mut PublicCommandRehearsal,
    project: &Path,
    parent: &Path,
    observation_directory: &Path,
    target_directory: &Path,
    destination: &str,
    checked: CheckedProducts,
) -> AcceptedExport {
    let manifest_digest = checked.headless.manifest_digest();
    let content_digest = checked.headless.content_digest();
    assert_eq!(manifest_digest, checked.desktop.manifest_digest());
    assert_eq!(content_digest, checked.desktop.content_digest());
    let build = cargo::build_in_target_directory(project, &checked.headless, target_directory)
        .unwrap()
        .into_receipt();
    let prepared = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(parent).unwrap(),
        RelativeComponent::new(destination).unwrap(),
        checked.headless,
        build,
    ))
    .unwrap();
    drop(checked.desktop);
    assert_eq!(prepared.plan().startup_content_digest(), content_digest);
    assert_eq!(
        manifest_member_digest(prepared.plan().members()),
        manifest_digest
    );

    let command_observation = public_commands.expect_export_success(
        project,
        observation_directory,
        [
            OsString::from("export"),
            OsString::from("."),
            OsString::from("--parent"),
            parent.as_os_str().to_owned(),
            OsString::from("--destination"),
            OsString::from(destination),
        ],
    );
    let root = parent.join(destination);
    validate_export_command_observation(
        &command_observation,
        prepared.plan().members(),
        prepared.plan().startup_content_digest(),
        prepared.plan().build_input_digest(),
        prepared.plan().member_plan_digest(),
        manifest_digest,
    );
    let plan = ExportPlanEvidence {
        startup_content_digest: prepared.plan().startup_content_digest(),
        build_input_digest: prepared.plan().build_input_digest(),
        member_plan_digest: prepared.plan().member_plan_digest(),
        members: prepared.plan().members().to_vec().into_boxed_slice(),
    };
    drop(prepared);
    let accepted = AcceptedExport {
        root,
        destination: destination.to_owned(),
        plan,
        products: command_observation
            .get("products")
            .cloned()
            .expect("the export command observation contains products"),
    };
    accepted.verify_unchanged();
    accepted
}

fn validate_export_command_observation(
    value: &Value,
    members: &[ProjectExportMemberReceipt],
    startup_content_digest: ContentDigest,
    build_input_digest: ContentDigest,
    member_plan_digest: ContentDigest,
    manifest_digest: ContentDigest,
) {
    assert_eq!(value.get("version").and_then(Value::as_u64), Some(1));
    assert_eq!(
        value.get("startup_content"),
        Some(&digest_value(startup_content_digest))
    );
    assert_eq!(
        value.get("build_input"),
        Some(&digest_value(build_input_digest))
    );
    assert_eq!(
        value.get("member_plan"),
        Some(&digest_value(member_plan_digest))
    );
    let products = value
        .get("products")
        .and_then(Value::as_array)
        .expect("the export command observation contains product receipts");
    assert_eq!(products.len(), 2);
    for (product, target) in products.iter().zip([
        ProjectExportBinaryTarget::Headless,
        ProjectExportBinaryTarget::Desktop,
    ]) {
        assert_eq!(
            product.get("target").and_then(Value::as_str),
            Some(target.cargo_target_name())
        );
        assert_eq!(
            product.get("profile").and_then(Value::as_str),
            Some(acceptance::product_profile(target))
        );
        assert_eq!(
            product.get("executable"),
            Some(&digest_value(export_executable_digest(members, target)))
        );
        assert_eq!(
            product.get("manifest"),
            Some(&digest_value(manifest_digest))
        );
        assert_eq!(
            product.get("content"),
            Some(&digest_value(startup_content_digest))
        );
        assert!(
            product
                .get("attempt")
                .and_then(Value::as_str)
                .is_some_and(|attempt| !attempt.is_empty() && attempt.len() <= 128)
        );
        assert!(product.get("observation").is_some_and(is_digest_value));
    }
}

fn export_executable_digest(
    members: &[ProjectExportMemberReceipt],
    target: ProjectExportBinaryTarget,
) -> ContentDigest {
    let kind = match target {
        ProjectExportBinaryTarget::Headless => ProjectExportMemberKind::HeadlessExecutable,
        ProjectExportBinaryTarget::Desktop => ProjectExportMemberKind::DesktopExecutable,
    };
    members
        .iter()
        .find(|member| member.kind() == kind)
        .expect("the export plan contains both product executables")
        .digest()
}

fn is_digest_value(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.len() == 2
        && object.get("length").and_then(Value::as_u64).is_some()
        && object
            .get("blake3")
            .and_then(Value::as_str)
            .is_some_and(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
}

fn manifest_member_digest(members: &[ProjectExportMemberReceipt]) -> ContentDigest {
    members
        .iter()
        .find(|member| {
            member.kind()
                == ProjectExportMemberKind::StartupContent(
                    StartupContentMemberKind::ProjectManifest,
                )
        })
        .expect("the export plan must retain its manifest")
        .digest()
}

fn checked_value(manifest_digest: ContentDigest, content_digest: ContentDigest) -> Value {
    json!({
        "manifest": digest_value(manifest_digest),
        "content": digest_value(content_digest),
    })
}

fn product_value(product: &ProductProcessObservationReceipt) -> Value {
    json!({
        "attempt": product.attempt(),
        "target": product.target().cargo_target_name(),
        "profile": product.profile(),
        "executable": digest_value(product.executable_digest()),
        "manifest": digest_value(product.manifest_digest()),
        "content": digest_value(product.startup_content_digest()),
        "observation": digest_value(product.observation_digest()),
    })
}

fn edit_startup_scene(project: &Path) -> ContentDigest {
    let path = project.join("scenes/startup.scene.json");
    let mut document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let value = document
        .pointer_mut(
            "/payload/entities/0/components/nara.render.Camera2d/value/value/clear_color/value/r/value",
        )
        .expect("the starter exposes one documented clear-color edit");
    assert_eq!(value.as_f64(), Some(0.06));
    *value = json!(EDITED_CLEAR_COLOR_R);
    let mut bytes = serde_json::to_vec_pretty(&document).unwrap();
    bytes.push(b'\n');
    fs::write(&path, &bytes).unwrap();
    ContentDigest::of_bytes(&bytes)
}

fn generate_lockfile(project: &Path, working_directory: &Path) {
    let output = Command::new(env!("CARGO"))
        .current_dir(working_directory)
        .args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo generate-lockfile failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn digest_value(digest: ContentDigest) -> Value {
    json!({
        "length": digest.length(),
        "blake3": acceptance::digest_hex(digest),
    })
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

struct TrajectoryRoot {
    target: PathBuf,
    path: PathBuf,
}

impl TrajectoryRoot {
    fn new() -> Self {
        let target = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
            .expect("the workspace target directory must exist");
        let sequence = NEXT_TRAJECTORY.fetch_add(1, Ordering::Relaxed);
        let path = target.join(format!(
            "nara-fper-trajectory-{}-{sequence}",
            std::process::id()
        ));
        assert!(path.starts_with(&target));
        match fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("failed to remove the stale trajectory root: {error}"),
        }
        fs::create_dir(&path).unwrap();
        Self { target, path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn shared_cargo_target(&self) -> PathBuf {
        self.target.join("nara-fper-product-cache")
    }
}

impl Drop for TrajectoryRoot {
    fn drop(&mut self) {
        if self.path.starts_with(&self.target) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
