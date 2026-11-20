use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    ProductRecipe,
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, AssetSourcePath, StableAssetId,
    },
    asset_build::ArtifactContentDigest,
    image::ImageImportSettings,
    project_content::CookedProductSource,
    project_host::{
        ProjectContentBudgetKind, ProjectContentLimits, ProjectContentLoader,
        ingest_project_manifest, resolve_product_recipe,
    },
    project_tooling::cargo_build::{CargoBuildLimits, CargoBuildTools},
    project_workflow::cargo::CargoBuildSelection,
    scene::SceneDocument,
};

use super::*;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    parent: PathBuf,
    project: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let parent = temporary.join(format!(
            "nara-project-cook-package-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&parent).unwrap();
        let fixture = Self {
            project: parent.join("project"),
            parent,
        };
        for directory in ["assets", "prefabs", "scenes", "src/nested"] {
            fs::create_dir_all(fixture.project.join(directory)).unwrap();
        }
        for (name, bytes) in [
            ("Cargo.toml", "manifest"),
            ("Cargo.lock", "lock"),
            ("src/main.rs", "fn main() {}"),
            ("src/nested/module.rs", "pub const VALUE: u8 = 1;"),
            ("src/nested/notes.txt", "not a Rust input"),
        ] {
            fixture.write(name, bytes);
        }
        fixture.write(
            "nara.toml",
            r#"schema_version = 1
[project]
source_universe = '39f9d09d-63e3-5fc4-8e3f-71e84893cb8d'
native_document_identity = 'project_location'
name = "Project cook package test"
[authoring]
state = "ready"
schema_bundle = "schema.bundle.json"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[capabilities]
requested = ["runtime-2d"]
"#,
        );
        fixture.write(
            "scenes/startup.scene.json",
            &SceneDocument::default().to_json_string().unwrap(),
        );
        fixture
    }

    fn write(&self, name: &str, bytes: &str) {
        fs::write(self.project.join(name), bytes).unwrap();
    }

    fn delivery(
        &self,
    ) -> (
        DirectoryCapability,
        ProjectContentLoader,
        ProjectContentDeliveryReceipt,
    ) {
        let root = open_project_root(&self.project).unwrap();
        let manifest = root
            .open_file_exact(&relative("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
        self.write(
            "schema.bundle.json",
            &plan
                .schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        );
        let loader =
            ProjectContentLoader::with_limits(root.share(), ProjectContentLimits::default())
                .unwrap();
        let receipt = loader.load_delivery(&candidate, &plan).unwrap();
        (root, loader, receipt)
    }

    fn request(&self, inputs: Vec<RelativePath>) -> CargoBuildRequest {
        let tools = self.parent.join("tools");
        fs::create_dir(&tools).unwrap();
        let cargo = tools.join(format!("cargo{}", std::env::consts::EXE_SUFFIX));
        let rustc = tools.join(format!("rustc{}", std::env::consts::EXE_SUFFIX));
        for tool in [&cargo, &rustc] {
            fs::write(tool, b"tool identity fixture; never executed").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(tool, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        CargoBuildRequest::new(
            &self.project,
            relative("Cargo.toml").unwrap(),
            CargoBuildSelection::new(
                "fixture",
                "player",
                "dev",
                "x86_64-pc-windows-msvc",
                Vec::<String>::new(),
                false,
            )
            .unwrap(),
            CargoBuildTools::new(cargo, rustc).unwrap(),
            inputs,
            CargoBuildLimits::default(),
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let parent = self.parent.canonicalize().unwrap();
        let relative = parent.strip_prefix(&temporary).unwrap();
        assert_eq!(relative.components().count(), 1);
        assert!(
            relative
                .to_str()
                .unwrap()
                .starts_with("nara-project-cook-package-")
        );
        fs::remove_dir_all(parent).unwrap();
    }
}

fn raw(name: &str, bytes: Option<&[u8]>) -> ArtifactBuildObservation {
    ArtifactBuildObservation::Raw {
        path: AssetSourcePath::new(name).unwrap(),
        digest: bytes.map(ArtifactContentDigest::of_bytes),
    }
}

#[test]
fn source_capture_deduplicates_equal_facts_and_checks_absent_inputs() {
    let fixture = Fixture::new();
    fixture.write("assets/build-input.txt", "original");
    let (root, _loader, receipt) = fixture.delivery();
    let observations = vec![
        raw("assets/build-input.txt", Some(b"original")),
        raw("assets/build-input.txt", Some(b"original")),
        raw("assets/optional.txt", None),
        raw("assets/optional.txt", None),
    ];
    let member_count = receipt.members().len();
    let selection =
        ProjectCookSourceSelection::capture(&root, receipt, observations.clone(), None).unwrap();
    assert_eq!(selection.files.len(), member_count + 2);
    assert_eq!(selection.observations(), observations);
    selection.verify(&fixture.project).unwrap();

    fixture.write("assets/optional.txt", "newly present");
    assert!(matches!(
        selection.verify(&fixture.project),
        Err(ProjectCookError::Filesystem(FsError::TargetStateMismatch))
    ));
}

#[test]
fn selected_image_scan_rejects_sidecar_bytes_changed_after_the_file_check() {
    let fixture = Fixture::new();
    let (root, loader, receipt) = fixture.delivery();
    let manifest = root
        .open_file_exact(&relative("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let path = "assets/selected.png.meta";
    let id = StableAssetId::parse_str("257bb27b-5c5f-559d-8283-f498450385be").unwrap();
    let mut bytes = AssetMeta::new(
        AssetMetaIdentity::new(
            id,
            AssetPath::new("selected.png").unwrap(),
            AssetSourceKind::Image,
        ),
        ImageImportSettings::default(),
    )
    .to_json_string()
    .unwrap();
    fixture.write(path, &bytes);
    let product_sources = CookedProductSources::new(
        &loader,
        &candidate,
        [(
            id,
            CookedProductSource {
                path: AssetSourcePath::new("assets/selected.png").unwrap(),
                metadata: Some(ContentDigest::of_bytes(bytes.as_bytes())),
            },
        )]
        .into_iter()
        .collect(),
    );
    let selection = ProjectCookSourceSelection::capture(
        &root,
        receipt,
        vec![raw(path, Some(bytes.as_bytes()))],
        product_sources,
    )
    .unwrap();
    selection.verify(&fixture.project).unwrap();
    for file in &selection.files {
        file.verify(&selection.root).unwrap();
    }
    bytes.push('\n');
    fixture.write(path, &bytes);
    let error = selection
        .product_sources
        .as_ref()
        .unwrap()
        .verify(&selection.root)
        .unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.asset-metadata-source-changed"
    }));
}

#[test]
fn source_capture_rejects_conflicting_presence_and_digests() {
    let fixture = Fixture::new();
    fixture.write("assets/build-input.txt", "original");
    let (root, _loader, receipt) = fixture.delivery();
    for conflict in [
        raw("assets/build-input.txt", None),
        raw("assets/build-input.txt", Some(b"different")),
    ] {
        assert!(matches!(
            ProjectCookSourceSelection::capture(
                &root,
                receipt.clone(),
                vec![raw("assets/build-input.txt", Some(b"original")), conflict],
                None,
            ),
            Err(ProjectCookError::Invalid(_))
        ));
    }
    let member = &receipt.members()[0];
    let conflict = raw(member.path().as_str(), None);
    assert!(matches!(
        ProjectCookSourceSelection::capture(&root, receipt, vec![conflict], None),
        Err(ProjectCookError::Invalid(_))
    ));
}

#[test]
fn absent_observations_accept_only_not_found() {
    let fixture = Fixture::new();
    let root = open_project_root(&fixture.project).unwrap();
    let path = relative("assets/optional.txt").unwrap();
    let absent = SelectedFile::capture(&root, path.clone(), None).unwrap();
    absent.verify(&root).unwrap();
    fs::create_dir(fixture.project.join("assets/optional.txt")).unwrap();
    assert!(SelectedFile::capture(&root, path, None).is_err());
    assert!(absent.verify(&root).is_err());
}

#[test]
fn source_capture_refuses_stale_receipt_bytes_with_digest_length_bound() {
    let fixture = Fixture::new();
    let (root, _loader, receipt) = fixture.delivery();
    fixture.write("scenes/startup.scene.json", &"x".repeat(1024 * 1024));
    assert!(matches!(
        ProjectCookSourceSelection::capture(&root, receipt, Vec::new(), None),
        Err(ProjectCookError::Filesystem(
            FsError::ByteLimitExceeded { .. }
        ))
    ));
}

#[test]
fn selection_rejects_same_bytes_in_a_replaced_file_and_a_replaced_root() {
    let fixture = Fixture::new();
    fixture.write("assets/build-input.txt", "original");
    let (root, _loader, receipt) = fixture.delivery();
    let selection = ProjectCookSourceSelection::capture(
        &root,
        receipt,
        vec![raw("assets/build-input.txt", Some(b"original"))],
        None,
    )
    .unwrap();
    fs::rename(
        fixture.project.join("assets/build-input.txt"),
        fixture.project.join("assets/retired.txt"),
    )
    .unwrap();
    fixture.write("assets/build-input.txt", "original");
    assert!(matches!(
        selection.verify(&fixture.project),
        Err(ProjectCookError::Filesystem(
            FsError::IdentityMismatch { .. }
        ))
    ));
    fs::rename(&fixture.project, fixture.parent.join("retired-project")).unwrap();
    fs::create_dir(&fixture.project).unwrap();
    assert!(matches!(
        selection.verify(&fixture.project),
        Err(ProjectCookError::Invalid(_))
    ));
}

#[test]
fn source_metadata_uses_original_retained_budget_without_claiming_a_generation() {
    let fixture = Fixture::new();
    let (root, loader, receipt) = fixture.delivery();
    let kind = ProjectContentBudgetKind::RetainedBytes;
    let before = loader.budget_snapshot();
    let observations = vec![raw("assets/optional.txt", None)];
    let selection =
        ProjectCookSourceSelection::capture(&root, receipt.clone(), observations.clone(), None)
            .unwrap();
    let retained = loader.budget_snapshot().active(kind) - before.active(kind);
    assert!(retained > 0);
    assert_eq!(
        loader.budget_snapshot().active_generations(),
        before.active_generations()
    );
    drop(selection);
    assert_eq!(loader.budget_snapshot().active(kind), before.active(kind));

    let allowance = loader.budget_host().limits().limit(kind) - before.active(kind) - retained;
    let exact = receipt.reserve_consumer_retained(allowance).unwrap();
    let selection =
        ProjectCookSourceSelection::capture(&root, receipt.clone(), observations.clone(), None)
            .unwrap();
    drop(selection);
    drop(exact);
    let one_short = receipt.reserve_consumer_retained(allowance + 1).unwrap();
    assert!(matches!(
        ProjectCookSourceSelection::capture(&root, receipt, observations, None),
        Err(ProjectCookError::Source(_))
    ));
    drop(one_short);
    assert_eq!(loader.budget_snapshot().active(kind), 0);
}

#[test]
fn default_inventory_selects_only_rust_and_root_cargo_inputs() {
    let fixture = Fixture::new();
    fixture.write("build.rs", "fn main() {}");
    fixture.write("assets/compiler.wgsl", "shader");
    let inventory = project_rust_input_inventory(&fixture.project).unwrap();
    let mut expected = [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/main.rs",
        "src/nested/module.rs",
    ]
    .map(|name| relative(name).unwrap());
    expected.sort_unstable_by(compare_paths);
    assert_eq!(inventory, expected);
}

#[test]
fn declared_code_keeps_extra_inputs_and_detects_default_set_changes() {
    let fixture = Fixture::new();
    fixture.write("assets/compiler.wgsl", "shader");
    let (root, _loader, receipt) = fixture.delivery();
    let source = ProjectCookSourceSelection::capture(&root, receipt, Vec::new(), None).unwrap();
    let mut inputs = project_rust_input_inventory(&fixture.project).unwrap();
    inputs.push(relative("assets/compiler.wgsl").unwrap());
    let request = fixture.request(inputs);
    let code = ProjectCodeSelection::capture(&source, &request).unwrap();
    code.verify(&source).unwrap();

    fixture.write("assets/compiler.wgsl", "change");
    assert!(code.verify(&source).is_err());
    fixture.write("assets/compiler.wgsl", "shader");
    code.verify(&source).unwrap();

    fixture.write("src/added.rs", "pub struct Added;");
    let error = code.verify(&source).unwrap_err();
    assert!(
        matches!(
            error,
            ProjectCookError::Cargo(CargoBuildFailure::MissingDeclaredInput)
        ),
        "{error:?}"
    );
    fs::remove_file(fixture.project.join("src/added.rs")).unwrap();
    code.verify(&source).unwrap();
    fs::remove_file(fixture.project.join("src/nested/module.rs")).unwrap();
    assert!(code.verify(&source).is_err());
}

#[test]
fn undeclared_rust_file_rejects_without_inferring_dependency_inputs() {
    let fixture = Fixture::new();
    let (root, _loader, receipt) = fixture.delivery();
    let source = ProjectCookSourceSelection::capture(&root, receipt, Vec::new(), None).unwrap();
    let inputs = ["Cargo.toml", "Cargo.lock", "src/main.rs"]
        .map(|name| relative(name).unwrap())
        .to_vec();
    let request = fixture.request(inputs);
    let error = ProjectCodeSelection::capture(&source, &request)
        .err()
        .unwrap();
    assert!(
        matches!(
            error,
            ProjectCookError::Cargo(CargoBuildFailure::MissingDeclaredInput)
        ),
        "{error:?}"
    );
}

#[test]
fn inventory_refuses_multiple_links_and_excessive_depth() {
    let fixture = Fixture::new();
    fs::hard_link(
        fixture.project.join("src/main.rs"),
        fixture.parent.join("main-alias.rs"),
    )
    .unwrap();
    assert!(project_rust_input_inventory(&fixture.project).is_err());
    fs::remove_file(fixture.parent.join("main-alias.rs")).unwrap();
    let mut path = fixture.project.join("src");
    for _ in 0..MAX_RUST_DEPTH {
        path.push("d");
        fs::create_dir(&path).unwrap();
    }
    assert!(project_rust_input_inventory(&fixture.project).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn inventory_refuses_symlinked_files_and_directories() {
    let fixture = Fixture::new();
    std::os::unix::fs::symlink(
        fixture.project.join("src/main.rs"),
        fixture.project.join("src/linked.rs"),
    )
    .unwrap();
    assert!(project_rust_input_inventory(&fixture.project).is_err());
    fs::remove_file(fixture.project.join("src/linked.rs")).unwrap();
    std::os::unix::fs::symlink(
        fixture.project.join("assets"),
        fixture.project.join("src/escape"),
    )
    .unwrap();
    assert!(project_rust_input_inventory(&fixture.project).is_err());
}
