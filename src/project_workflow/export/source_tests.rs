use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    ProductRecipe,
    fs::{DirectoryCapability, FsError, RelativeComponent, RelativePath},
    project_host::{StartupContentClosureReceipt, ingest_project_manifest, resolve_product_recipe},
    project_tooling::host_fs,
    project_workflow::{ProjectCheckRequest, check_project},
    scene::SceneDocument,
};

use super::{
    ProjectExportBinaryTarget, ProjectExportBuildError, ProjectExportBuildPreparation,
    ProjectExportBuildReceipt, ProjectExportCargoArtifact, ProjectExportPlanError,
    ProjectExportProductAgreement, ProjectExportRequest, prepare_project_export,
    revalidate_delivery_sources,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn export_revalidation_rejects_a_changed_nonstartup_delivery_scene_before_staging() {
    let project = SourceProject::new();
    let delivery = check_project(ProjectCheckRequest::new(
        host_fs::readable_directory(&project.root).unwrap(),
        ProductRecipe::new(),
    ))
    .unwrap();
    assert_eq!(
        delivery
            .scene_roots()
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        ["a.scene.json", "b.scene.json"]
    );
    assert!(
        delivery
            .members()
            .iter()
            .any(|member| { member.path().as_str() == "scenes/b.scene.json" })
    );
    assert!(
        delivery
            .startup_content()
            .members()
            .iter()
            .all(|member| { member.path().as_str() != "scenes/b.scene.json" })
    );
    revalidate_delivery_sources(&delivery).unwrap();
    let root_entries = entries(&project.root);
    let scene_entries = entries(&project.root.join("scenes"));
    let startup_bytes = fs::read(project.root.join("scenes/a.scene.json")).unwrap();
    let manifest_bytes = fs::read(project.root.join("nara.toml")).unwrap();

    // Keep B valid, the byte length equal, and startup unchanged: delivery owns exact bytes.
    let second_scene = project.root.join("scenes/b.scene.json");
    let mut changed_bytes = fs::read(&second_scene).unwrap();
    *changed_bytes
        .iter_mut()
        .find(|byte| **byte == b' ')
        .unwrap() = b'\t';
    fs::write(&second_scene, changed_bytes).unwrap();

    let error = revalidate_delivery_sources(&delivery).unwrap_err();
    match error {
        ProjectExportPlanError::ContentChanged { path, source } => {
            assert_eq!(path.as_str(), "scenes/b.scene.json");
            assert!(matches!(source, FsError::DigestMismatch { .. }));
        }
        other => panic!("unexpected export source error: {other:?}"),
    }
    assert_eq!(
        fs::read(project.root.join("scenes/a.scene.json")).unwrap(),
        startup_bytes
    );
    assert_eq!(
        fs::read(project.root.join("nara.toml")).unwrap(),
        manifest_bytes
    );
    assert_eq!(entries(&project.root), root_entries);
    assert_eq!(entries(&project.root.join("scenes")), scene_entries);
}

#[test]
fn export_rejects_a_build_agreement_for_a_different_freshly_checked_startup() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let original = project.check(&root);
    let build = project
        .synthetic_build(
            original.startup_content(),
            &original.schema_fingerprint().to_string(),
        )
        .unwrap();
    build.revalidate(original.startup_content()).unwrap();
    let root_entries = entries(&project.root);
    let second_bytes = fs::read(project.root.join("scenes/b.scene.json")).unwrap();
    let mut startup_bytes = fs::read(project.root.join("scenes/a.scene.json")).unwrap();
    startup_bytes.push(b'\n');
    fs::write(project.root.join("scenes/a.scene.json"), startup_bytes).unwrap();

    let current = project.check(&root);
    assert_eq!(original.manifest_digest(), current.manifest_digest());
    assert_eq!(original.schema_fingerprint(), current.schema_fingerprint());
    assert_ne!(
        original.startup_content().content_digest(),
        current.startup_content().content_digest()
    );
    assert_ne!(
        original.startup_content().closure_plan_digest(),
        current.startup_content().closure_plan_digest()
    );
    revalidate_delivery_sources(&current).unwrap();
    let error = prepare_project_export(ProjectExportRequest::new(
        host_fs::writable_directory(&project.root).unwrap(),
        RelativeComponent::new("candidate").unwrap(),
        current,
        build,
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportPlanError::Build(ProjectExportBuildError::ProductAgreementMismatch {
            target: ProjectExportBinaryTarget::Headless
        })
    ));
    assert_eq!(entries(&project.root), root_entries);
    assert_eq!(
        fs::read(project.root.join("scenes/b.scene.json")).unwrap(),
        second_bytes
    );
}

#[test]
fn startup_build_agreement_does_not_require_observing_nonstartup_delivery_content() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let original = project.check(&root);
    let build = project
        .synthetic_build(
            original.startup_content(),
            &original.schema_fingerprint().to_string(),
        )
        .unwrap();
    let root_entries = entries(&project.root);
    let mut second_bytes = fs::read(project.root.join("scenes/b.scene.json")).unwrap();
    second_bytes.push(b'\n');
    fs::write(project.root.join("scenes/b.scene.json"), second_bytes).unwrap();

    let current = project.check(&root);
    assert_ne!(original.delivery_digest(), current.delivery_digest());
    assert_eq!(
        original.startup_content().closure_plan_digest(),
        current.startup_content().closure_plan_digest()
    );
    // Product agreement is intentionally startup-scoped; complete delivery validation remains a
    // separate export gate and must reject this changed non-startup source.
    build.revalidate(current.startup_content()).unwrap();
    revalidate_delivery_sources(&current).unwrap();
    assert!(matches!(
        revalidate_delivery_sources(&original),
        Err(ProjectExportPlanError::ContentChanged { ref path, .. })
            if path.as_str() == "scenes/b.scene.json"
    ));
    assert_eq!(entries(&project.root), root_entries);
}

#[test]
fn build_binding_rejects_a_product_schema_fingerprint_different_from_its_startup() {
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let checked = project.check(&root);
    let wrong_schema = "00".repeat(32);
    assert_ne!(checked.schema_fingerprint().to_string(), wrong_schema);
    let error = project
        .synthetic_build(checked.startup_content(), &wrong_schema)
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectExportBuildError::ProductAgreementMismatch {
            target: ProjectExportBinaryTarget::Headless
        }
    ));
    assert!(!project.root.join("candidate").exists());
}

struct SourceProject {
    root: PathBuf,
    temporary_parent: PathBuf,
}

impl SourceProject {
    fn check(
        &self,
        root: &DirectoryCapability,
    ) -> crate::project_host::ProjectContentDeliveryReceipt {
        check_project(ProjectCheckRequest::new(root.share(), ProductRecipe::new())).unwrap()
    }

    // These files and observation facts are synthetic. They test owner matching only; no product
    // process runs here. The compiled CLI trajectory separately proves actual executable startup.
    fn synthetic_build(
        &self,
        startup: &StartupContentClosureReceipt,
        schema_fingerprint: &str,
    ) -> Result<ProjectExportBuildReceipt, ProjectExportBuildError> {
        fs::create_dir_all(self.root.join("src/bin")).unwrap();
        fs::create_dir_all(self.root.join("build-products")).unwrap();
        for (path, bytes) in [
            (
                "Cargo.toml",
                "[package]\nname = \"owner-test\"\nversion = \"0.1.0\"\n",
            ),
            ("Cargo.lock", "version = 4\n"),
            ("src/lib.rs", "// Synthetic build input.\n"),
            ("src/bin/headless.rs", "fn main() {}\n"),
            ("src/bin/desktop.rs", "fn main() {}\n"),
        ] {
            fs::write(self.root.join(path), bytes).unwrap();
        }
        let preparation = ProjectExportBuildPreparation::capture(startup)?;
        let root = startup.project_root();
        let [(headless, headless_agreement), (desktop, desktop_agreement)] = [
            ProjectExportBinaryTarget::Headless,
            ProjectExportBinaryTarget::Desktop,
        ]
        .map(|target| {
            let path = format!("build-products/{}", target.cargo_target_name());
            fs::write(self.root.join(&path), format!("synthetic {target:?} bytes")).unwrap();
            let file = root.open_file(&RelativePath::new(path).unwrap()).unwrap();
            let digest = file.digest(1024).unwrap();
            let artifact = ProjectExportCargoArtifact::new(
                target,
                "synthetic-owner-package",
                "x86_64-unknown-linux-gnu",
                "release",
                ["desktop".to_owned()],
                file,
            )
            .unwrap();
            let agreement = ProjectExportProductAgreement::new(
                target,
                digest,
                startup.manifest_digest(),
                startup.content_digest(),
                startup.closure_plan_digest(),
                startup.runtime_schema_bundle_digest().unwrap(),
                schema_fingerprint,
                crate::fs::ContentDigest::of_bytes(b"synthetic observation facts"),
            )
            .unwrap();
            (artifact, agreement)
        });
        preparation.bind([headless, desktop], [headless_agreement, desktop_agreement])
    }

    fn new() -> Self {
        let temporary_parent = std::env::temp_dir().canonicalize().unwrap();
        let root = temporary_parent.join(format!(
            "nara_export_sources_{}_{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let project = Self {
            root,
            temporary_parent,
        };
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(project.root.join(directory)).unwrap();
        }
        fs::write(
            project.root.join("nara.toml"),
            r#"schema_version = 1

[project]
name = "Export Source Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "a.scene.json"

[content]
scenes = ["b.scene.json"]

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();
        let scene = SceneDocument::default().to_json_string().unwrap();
        for name in ["a.scene.json", "b.scene.json"] {
            fs::write(project.root.join("scenes").join(name), &scene).unwrap();
        }
        let root = host_fs::readable_directory(&project.root).unwrap();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project
    }
}

impl Drop for SourceProject {
    fn drop(&mut self) {
        let relative = self.root.strip_prefix(&self.temporary_parent).unwrap();
        assert_eq!(relative.components().count(), 1);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn entries(directory: &Path) -> Vec<std::ffi::OsString> {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
