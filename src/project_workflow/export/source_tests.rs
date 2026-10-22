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

#[derive(nara_ecs::Component, nara_reflect::PersistentComponent)]
#[nara(
    id = "test.export.SourceText",
    version = 1,
    alias = "Source text",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct SourceText {
    #[nara(id = "source", alias = "Source", capabilities(scene, asset_ref))]
    pub source: nara_asset::AssetRef,
}

nara_reflect::persistent_components! { pub mod source_text_components = [SourceText]; }

#[derive(Default)]
struct SourceTextPlugin;

const SOURCE_TEXT_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::new(
        nara_reflect::ComponentSchemaOwnerId::new("test.export.source-text"),
        nara_app::PluginSchemaProviderId::new("test.export.source-text.schema"),
        nara_reflect::ComponentSchemaProviderBindingId::new("test.export.source-text.native", 1),
        source_text_components::initial_catalog,
        source_text_components::register,
    );

impl nara_app::Plugin for SourceTextPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        static DECLARATION: nara_app::PluginDeclaration = nara_app::PluginDeclaration::new(
            nara_app::PluginId::new("test.export.source-text"),
            nara_app::PluginCategory::Runtime,
        )
        .provides_schema(&[SOURCE_TEXT_PROVIDER.id()]);
        &DECLARATION
    }
    fn build(&self, _: &mut nara_app::App) -> Result<(), nara_app::PluginError> {
        Ok(())
    }
}

struct SourceTextDomain;

impl crate::project_host::ProjectAssetDomain for SourceTextDomain {
    type Settings = ();
    type Product = Vec<u8>;
    const SOURCE_KIND: &'static str = "test.export.text";
    fn limits() -> crate::project_host::ProjectAssetLimits {
        let bytes = nara_core::ByteLimit::new(1024).unwrap();
        crate::project_host::ProjectAssetLimits::new(bytes, bytes, bytes)
    }
    fn prepare(
        source: &mut crate::project_host::ProjectAssetSource<'_>,
    ) -> Result<Self::Product, crate::project_host::ProjectContentError> {
        source.metadata().admit::<Self::Settings>().unwrap();
        Ok(source
            .read_dependency(
                &nara_asset::AssetPath::new("suffix.txt").unwrap(),
                nara_core::ByteLimit::new(1024).unwrap(),
            )?
            .to_vec())
    }
}

fn source_text_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(
            crate::SchemaContribution::<SourceTextPlugin>::for_default([SOURCE_TEXT_PROVIDER])
                .unwrap(),
        )
        .unwrap()
        .add_asset_domain::<SourceTextDomain>()
        .unwrap()
}

#[test]
fn domain_sources_export_exact_members_and_reject_changed_or_missing_dependencies() {
    use nara_reflect::PersistentComponentProvider;
    let project = SourceProject::new();
    let root = host_fs::readable_directory(&project.root).unwrap();
    let metadata = nara_asset::AssetMeta::new(
        nara_asset::AssetMetaIdentity::new(
            nara_asset::StableAssetId::parse_str("4bc478ea-9882-49e8-91f1-62a2d5e9dc07").unwrap(),
            nara_asset::AssetPath::new("text.txt").unwrap(),
            nara_asset::AssetSourceKind::Other("test.export.text".to_owned()),
        ),
        (),
    );
    fs::write(
        project.root.join("assets/text.txt.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(project.root.join("assets/text.txt"), b"main source").unwrap();
    fs::write(project.root.join("assets/suffix.txt"), b"original").unwrap();
    let component = SourceText {
        source: nara_asset::AssetRef::path("text.txt").unwrap(),
    };
    let scene = SceneDocument::from_ordered_roots([nara_scene::SceneEntityRecord::new(
        nara_identity::SceneEntityId::new("text").unwrap(),
    )
    .with_component(
        nara_reflect::ComponentTypeId::new("test.export.SourceText"),
        nara_scene::SceneComponentRecord::new(
            nara_reflect::ComponentSchemaVersion::ONE,
            component.__encode_persistent_component().unwrap(),
        ),
    )]);
    fs::write(
        project.root.join("scenes/a.scene.json"),
        scene.to_json_string().unwrap(),
    )
    .unwrap();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, source_text_recipe()).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let check =
        || check_project(ProjectCheckRequest::new(root.share(), source_text_recipe())).unwrap();
    let delivery = check();
    assert_eq!(
        delivery
            .members()
            .iter()
            .filter(|member| member.path().as_str() == "assets/suffix.txt")
            .count(),
        1
    );
    let prepare = |name: &str| {
        let build = project
            .synthetic_build(
                delivery.startup_content(),
                &delivery.schema_fingerprint().to_string(),
            )
            .unwrap();
        prepare_project_export(ProjectExportRequest::new(
            host_fs::writable_directory(&project.root).unwrap(),
            RelativeComponent::new(name).unwrap(),
            delivery.clone(),
            build,
        ))
    };
    let original = prepare("published").unwrap().publish();
    assert!(matches!(
        original,
        super::ProjectExportOutcome::Published(_)
    ));
    assert_eq!(
        fs::read(project.root.join("published/project/assets/suffix.txt")).unwrap(),
        b"original"
    );
    let prepared = prepare("changed").unwrap();
    fs::write(project.root.join("assets/suffix.txt"), b"modified").unwrap();
    assert!(matches!(
        prepared.publish(),
        super::ProjectExportOutcome::Rejected(_)
    ));
    assert!(!project.root.join("changed").exists());
    assert!(matches!(
        prepare("changed-before-prepare"),
        Err(ProjectExportPlanError::ContentChanged { .. })
    ));
    fs::remove_file(project.root.join("assets/suffix.txt")).unwrap();
    assert!(matches!(
        prepare("missing"),
        Err(ProjectExportPlanError::ContentChanged { .. })
    ));
    assert!(!project.root.join("missing").exists());
    assert_eq!(
        fs::read(project.root.join("published/project/assets/suffix.txt")).unwrap(),
        b"original"
    );
}

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
