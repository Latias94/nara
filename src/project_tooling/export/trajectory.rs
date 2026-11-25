use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use crate::{
    fs::{ContentDigest, RelativeComponent, RelativePath},
    project::ProjectPath,
    project_host::ProjectContentDeliveryReceipt,
    project_workflow::{
        LocalDependencyRelation, NaraDependencySource, ProjectCheckRequest,
        ProjectContentMemberKind, ProjectCreationOutcome, ProjectCreationRequest,
        ProjectExportBinaryTarget, ProjectExportMemberKind, ProjectExportMemberReceipt,
        ProjectExportOutcome, ProjectExportPlanReceipt, ProjectExportRequest, ProjectSlug,
        ProjectStarter, ProjectStartupSceneRepairOutcome, ProjectStartupSceneRepairRequest,
        check_project, prepare_desktop_2d_project, prepare_project_export, repair_startup_scene,
    },
};
use serde_json::{Value, json};

use super::{
    acceptance::{self, ProductProcessObservationReceipt},
    cargo,
};
use crate::project_tooling::host_fs;

#[path = "../../project_workflow/export/profile_fixture.rs"]
mod profile_fixture;

const PROJECT_SLUG: &str = "fresh-project-trajectory";
const REPLACEMENT_SCENE: &str = "repaired.scene.json";
const DESKTOP_STARTUP_SCENE: &str = "desktop-startup.scene.json";
const EDITED_CLEAR_COLOR_R: f64 = 0.14;
static NEXT_TRAJECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
fn missing_startup_scene_diagnostic_preserves_the_selected_profile() {
    for profile in [None, Some("tools".to_owned()), Some("a".repeat(128))] {
        let root = TrajectoryRoot::new();
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(root.path().join(directory)).unwrap();
        }
        let manifest = r#"schema_version = 1
[project]
source_universe = "796b54db-7d94-51a0-9ca8-a19d46ce7b50"
native_document_identity = "project_location"
name = "Startup Scene Repair"
stable_id = "startup-scene-repair"
version = "0.1.0"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"
[startup]
default_scene = "missing.scene.json"
[runtime]
preset = "local-headless"
fixed_timestep_seconds = 0.02
[capabilities]
requested = ["runtime-2d"]
"#;
        let manifest = match &profile {
            Some(profile) => format!(
                "{manifest}\n[profiles.{profile}.startup]\ndefault_scene = \"missing.scene.json\"\n"
            ),
            None => manifest.to_owned(),
        };
        fs::write(root.path().join("nara.toml"), manifest).unwrap();
        let capability = host_fs::readable_directory(root.path()).unwrap();
        let manifest = capability
            .open_file(&crate::fs::RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate =
            crate::project_host::ingest_project_manifest(&manifest, profile.as_deref()).unwrap();
        let plan =
            crate::project_host::resolve_product_recipe(&candidate, crate::ProductRecipe::new())
                .unwrap();
        fs::write(
            root.path().join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let error = crate::project_tooling::execute(
            crate::project_tooling::ProjectToolRequest::Check {
                project: root.path().to_owned(),
                profile: profile.clone(),
            },
            crate::ProductRecipe::new(),
        )
        .unwrap_err();
        let response =
            crate::project_tooling::protocol::encode_response("check", Err(error)).unwrap();
        let bytes = serde_json::to_vec(&response).unwrap();
        let failure = crate::project_tooling::protocol::decode_response(&bytes, "check")
            .unwrap()
            .unwrap_err();
        let hint = failure
            .repair_hint()
            .expect("missing startup retains admitted repair context");
        assert_eq!(hint.scene(), Some("missing.scene.json"));
        assert_eq!(hint.scenes_root(), Some("scenes"));
        assert_eq!(hint.profile_selected(), profile.is_some());
        let display = failure.to_string();
        assert!(
            display.contains("startup Scene `missing.scene.json` is missing"),
            "{display}"
        );
        match profile.as_deref() {
            Some("tools") => {
                assert_eq!(hint.profile(), Some("tools"));
                assert_eq!(failure.loss().withheld_repair_fields, 0);
                assert!(display.contains("for profile `tools`"));
            }
            Some(withheld) => {
                assert_eq!(hint.profile(), None);
                assert_eq!(failure.loss().withheld_repair_fields, 1);
                assert!(display.contains("for the selected profile"));
                assert!(!display.contains(withheld));
                assert!(!String::from_utf8_lossy(&bytes).contains(withheld));
            }
            None => {
                assert_eq!(hint.profile(), None);
                assert_eq!(failure.loss().withheld_repair_fields, 0);
                assert!(!display.contains("for profile"));
                assert!(!display.contains("for the selected profile"));
            }
        }
    }
}

#[test]
#[ignore = "runs nested Cargo builds and a real headless/Winit/WGPU product trajectory"]
fn fresh_project_export_repair_is_one_linked_product_trajectory() {
    let started = Instant::now();
    let root = TrajectoryRoot::new();
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let project_parent = root.path().join("projects");
    let export_parent = root.path().join("exports");
    let unrelated_cwd = root.path().join("unrelated-cwd");
    let cargo_target = root.target.clone();
    fs::create_dir_all(&project_parent).unwrap();
    fs::create_dir_all(&export_parent).unwrap();
    fs::create_dir_all(&unrelated_cwd).unwrap();
    let create_started = Instant::now();
    let checkout_root = host_fs::writable_directory(&checkout).unwrap();
    let parent_path = RelativePath::from_components(
        project_parent
            .strip_prefix(&checkout)
            .unwrap()
            .iter()
            .map(|name| RelativeComponent::new(name).unwrap()),
    )
    .unwrap();
    let parent = checkout_root.open_directory(&parent_path).unwrap();
    let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
        ProjectStarter::Desktop2dV1,
        parent,
        ProjectSlug::new(PROJECT_SLUG).unwrap(),
        NaraDependencySource::local(LocalDependencyRelation::new(
            checkout_root,
            None,
            Some(parent_path),
        )),
    ))
    .unwrap();
    let outcome = prepared.publish();
    let ProjectCreationOutcome::Created(creation) = outcome else {
        panic!("starter publication failed: {outcome:?}");
    };
    let create_elapsed_ms = elapsed_ms(create_started);
    let project = project_parent.join(PROJECT_SLUG);
    let created = check_project(ProjectCheckRequest::new(
        host_fs::readable_directory(&project).unwrap(),
        crate::ProductRecipe::new(),
    ))
    .unwrap();
    assert_eq!(
        creation.content_digest(),
        created.startup_content().content_digest()
    );
    let creation_evidence = json!({
        "starter": "desktop-2d/v1",
        "dependency_kind": "LocalCheckout",
        "author_guide": digest_value(ContentDigest::of_bytes(
            &fs::read(project.join("README.md")).unwrap(),
        )),
        "manifest": digest_value(created.manifest_digest()),
        "content": digest_value(created.startup_content().content_digest()),
    });
    let created_content_digest = created.startup_content().content_digest();
    drop(created);
    drop(creation);

    let edited_scene_digest = edit_startup_scene(&project);
    profile_fixture::install_profiled_level(&project);
    install_desktop_startup(&project);
    let checked_before = check_products(&project);
    assert_ne!(
        checked_before.headless.startup_content().content_digest(),
        created_content_digest
    );
    let desktop_startup_before = checked_before.desktop.startup_content().content_digest();
    generate_lockfile(&project, &unrelated_cwd);

    let first_run_started = Instant::now();
    let source_before = run_source_products(
        "before-source",
        &project,
        &unrelated_cwd,
        &cargo_target,
        &checked_before,
    );
    let first_run_elapsed_ms = elapsed_ms(first_run_started);

    let first_export_started = Instant::now();
    let before_export = export_project(
        &project,
        &export_parent,
        &cargo_target,
        "candidate-before-repair",
        checked_before,
    );
    let first_export_elapsed_ms = elapsed_ms(first_export_started);

    let replacement_scene = move_startup_scene(&project);
    assert_eq!(
        ContentDigest::of_bytes(&fs::read(&replacement_scene).unwrap()),
        edited_scene_digest
    );

    let repair_started = Instant::now();
    let missing = check_project(ProjectCheckRequest::new(
        host_fs::writable_directory(&project).unwrap(),
        crate::ProductRecipe::new(),
    ))
    .unwrap_err();
    let issue = missing
        .into_startup_scene_issue()
        .expect("moving the selected startup Scene must produce a repairable issue");
    assert_eq!(issue.selected_scene().as_str(), "startup.scene.json");
    assert_eq!(issue.scenes_path().as_str(), "scenes");
    let issue_manifest_digest = issue.manifest_digest();
    let issue_evidence = json!({
        "manifest": digest_value(issue_manifest_digest),
        "scenes_root": issue.scenes_path().as_str(),
        "selected_scene": issue.selected_scene().as_str(),
    });
    let outcome = repair_startup_scene(ProjectStartupSceneRepairRequest::new(
        issue,
        ProjectPath::new(REPLACEMENT_SCENE).unwrap(),
    ))
    .unwrap();
    let ProjectStartupSceneRepairOutcome::Repaired(repair) = outcome else {
        panic!("startup Scene repair failed its post-check: {outcome:?}");
    };
    let repair_elapsed_ms = elapsed_ms(repair_started);

    before_export.verify_unchanged();
    let checked_after = check_products(&project);
    let after_manifest_digest = checked_after.headless.manifest_digest();
    let after_content_digest = checked_after.headless.startup_content().content_digest();
    assert_ne!(after_manifest_digest, issue_manifest_digest);
    assert_eq!(
        repair.publication().previous_manifest_digest(),
        issue_manifest_digest
    );
    assert_eq!(
        repair.publication().manifest_digest(),
        after_manifest_digest
    );
    assert_eq!(
        repair.publication().replacement_scene_digest(),
        edited_scene_digest
    );
    assert_eq!(
        repair.startup_content().content_digest(),
        after_content_digest
    );
    assert_eq!(
        checked_after.desktop.startup_content().content_digest(),
        desktop_startup_before,
    );
    assert_eq!(
        checked_after
            .headless
            .members()
            .iter()
            .find(|member| {
                member.kind() == ProjectContentMemberKind::Scene
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
        "after-source",
        &project,
        &unrelated_cwd,
        &cargo_target,
        &checked_after,
    );
    let after_export = export_project(
        &project,
        &export_parent,
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
    headless: ProjectContentDeliveryReceipt,
    desktop: ProjectContentDeliveryReceipt,
}

fn check_products(project: &Path) -> CheckedProducts {
    let [headless, desktop] = ProjectExportRequest::check_deliveries(
        host_fs::readable_directory(project).unwrap(),
        &crate::ProductRecipe::new(),
    )
    .unwrap();
    assert_eq!(headless.manifest_digest(), desktop.manifest_digest());
    assert_ne!(
        headless.startup_content().content_digest(),
        desktop.startup_content().content_digest()
    );
    assert_ne!(
        headless.startup_content().closure_plan_digest(),
        desktop.startup_content().closure_plan_digest()
    );
    assert_ne!(headless.settings_lineage(), desktop.settings_lineage());
    assert_ne!(headless.delivery_digest(), desktop.delivery_digest());
    CheckedProducts { headless, desktop }
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
    let headless = acceptance::observe_prepared_product_command(
        attempt_prefix,
        acceptance::ExpectedProductObservation::new(
            ProjectExportBinaryTarget::Headless,
            &headless_path,
            headless_digest,
            checked.headless.startup_content(),
        )
        .unwrap(),
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
    let desktop = acceptance::observe_prepared_product_command(
        attempt_prefix,
        acceptance::ExpectedProductObservation::new(
            ProjectExportBinaryTarget::Desktop,
            &desktop_path,
            desktop_digest,
            checked.desktop.startup_content(),
        )
        .unwrap(),
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
    project: &Path,
    parent: &Path,
    target_directory: &Path,
    destination: &str,
    checked: CheckedProducts,
) -> AcceptedExport {
    let manifest_digest = checked.headless.manifest_digest();
    let content_digest = checked.headless.startup_content().content_digest();
    let build = cargo::build_in_target_directory(
        project,
        [
            checked.headless.startup_content(),
            checked.desktop.startup_content(),
        ],
        target_directory,
    )
    .unwrap()
    .into_receipt();
    let prepared = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(parent).unwrap(),
        RelativeComponent::new(destination).unwrap(),
        [checked.headless, checked.desktop],
        build,
        crate::ProductRecipe::new(),
    ))
    .unwrap();
    assert_eq!(
        prepared
            .plan()
            .startup_content_digest(ProjectExportBinaryTarget::Headless),
        content_digest
    );
    assert_eq!(
        manifest_member_digest(prepared.plan().members()),
        manifest_digest
    );

    let outcome = prepared.publish();
    let ProjectExportOutcome::Published(receipt) = outcome else {
        panic!("export publication failed: {outcome:?}");
    };
    let original = parent.join(destination);
    let root = parent.join(format!("{destination}-relocated"));
    #[cfg(windows)]
    let rename_deadline = Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match fs::rename(&original, &root) {
            Ok(()) => break,
            #[cfg(windows)]
            Err(error)
                if Instant::now() < rename_deadline
                    && (error.kind() == std::io::ErrorKind::PermissionDenied
                        || error.raw_os_error()
                            == Some(
                                windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION as i32,
                            )) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(error) => panic!("failed to relocate the published export {destination}: {error}"),
        }
    }
    assert!(!original.exists());
    profile_fixture::assert_profiled_level_images(&root.join("project"));
    let plan = receipt.plan();
    let acceptance = acceptance::accept(&root, plan).unwrap();
    assert_product_agreement(acceptance.products(), plan);
    let accepted = AcceptedExport {
        root,
        destination: destination.to_owned(),
        plan: ExportPlanEvidence {
            startup_content_digest: plan
                .startup_content_digest(ProjectExportBinaryTarget::Headless),
            build_input_digest: plan.build_input_digest(),
            member_plan_digest: plan.member_plan_digest(),
            members: plan.members().to_vec().into_boxed_slice(),
        },
        products: Value::Array(acceptance.products().iter().map(product_value).collect()),
    };
    accepted.verify_unchanged();
    accepted
}

fn assert_product_agreement(
    products: &[ProductProcessObservationReceipt; 2],
    plan: &ProjectExportPlanReceipt,
) {
    for (product, target) in products.iter().zip(ProjectExportBinaryTarget::ALL) {
        let agreement = plan.product_agreement(target);
        assert_eq!(product.target(), target);
        assert_eq!(
            product.executable_digest(),
            export_executable_digest(plan.members(), target),
        );
        assert_eq!(product.manifest_digest(), agreement.manifest_digest);
        assert_eq!(
            product.startup_content_digest(),
            agreement.startup_content_digest,
        );
        assert_eq!(
            product.startup_closure_plan_digest(),
            agreement.startup_closure_plan_digest,
        );
        assert_eq!(
            product.authoring_schema_bundle_digest(),
            agreement.authoring_schema_bundle_digest,
        );
        assert_eq!(
            product.schema_fingerprint(),
            agreement.schema_fingerprint.as_ref(),
        );
        assert!(product.observation_digest().length() > 0);
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

fn manifest_member_digest(members: &[ProjectExportMemberReceipt]) -> ContentDigest {
    members
        .iter()
        .find(|member| {
            member.kind()
                == ProjectExportMemberKind::Content(ProjectContentMemberKind::ProjectManifest)
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
        "closure_plan": digest_value(product.startup_closure_plan_digest()),
        "authoring_schema_bundle": digest_value(product.authoring_schema_bundle_digest()),
        "schema_fingerprint": product.schema_fingerprint(),
        "observation": digest_value(product.observation_digest()),
    })
}

fn edit_startup_scene(project: &Path) -> ContentDigest {
    let path = project.join("scenes/startup.scene.json");
    edit_scene_clear_color(&path, 0.06, EDITED_CLEAR_COLOR_R)
}

fn edit_scene_clear_color(path: &Path, original: f64, updated: f64) -> ContentDigest {
    let mut document: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let value = document
        .pointer_mut(
            "/payload/entities/0/components/nara.render.Camera2d/value/value/clear_color/value/r/value",
        )
        .expect("the starter exposes one documented clear-color edit");
    assert_eq!(value.as_f64(), Some(original));
    *value = json!(updated);
    let mut bytes = serde_json::to_vec_pretty(&document).unwrap();
    bytes.push(b'\n');
    fs::write(path, &bytes).unwrap();
    ContentDigest::of_bytes(&bytes)
}

fn install_desktop_startup(project: &Path) {
    let scenes = project.join("scenes");
    let path = scenes.join(DESKTOP_STARTUP_SCENE);
    fs::copy(scenes.join("startup.scene.json"), &path).unwrap();
    edit_scene_clear_color(&path, EDITED_CLEAR_COLOR_R, 0.28);
    profile_fixture::write_scene_sidecar(
        &scenes,
        DESKTOP_STARTUP_SCENE,
        "f12228c8-740f-4a2c-9ec1-bac91b72bf14",
    );
    let manifest_path = project.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        manifest_path,
        format!(
            "{manifest}\n[profiles.desktop.startup]\ndefault_scene = '{DESKTOP_STARTUP_SCENE}'\n"
        ),
    )
    .unwrap();
}

fn move_startup_scene(project: &Path) -> PathBuf {
    use nara_asset::{
        AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetSourceKind,
    };

    let scenes = project.join("scenes");
    let replacement = scenes.join(REPLACEMENT_SCENE);
    fs::rename(scenes.join("startup.scene.json"), &replacement).unwrap();
    let old_meta_path = scenes.join("startup.scene.json.meta");
    let original =
        AssetMetaIdentityCandidate::decode_json_bytes(&fs::read(&old_meta_path).unwrap()).unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            original.identity().stable_id(),
            AssetPath::new(REPLACEMENT_SCENE).unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    );
    fs::write(
        scenes.join(format!("{REPLACEMENT_SCENE}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::remove_file(old_meta_path).unwrap();
    replacement
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
}

impl Drop for TrajectoryRoot {
    fn drop(&mut self) {
        if self.path.starts_with(&self.target) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
