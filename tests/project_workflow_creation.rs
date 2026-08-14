#![cfg(feature = "project-workflow")]

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    project_workflow::{
        LocalDependencyParent, NaraDependencySource, ProjectCreationPlanErrorKind,
        ProjectCreationRequest, ProjectDirectory, ProjectMember, ProjectSlug,
        prepare_desktop_2d_project,
    },
};

const GIT_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(1);

#[test]
fn desktop_2d_v1_is_one_closed_prevalidated_project_plan() {
    let root = TestRoot::new("closed-plan");
    let parent = root.writable_capability();
    let source = NaraDependencySource::immutable_git(
        "https://github.com/nara-engine/nara.git",
        GIT_REVISION,
    )
    .unwrap();
    let request =
        ProjectCreationRequest::new(parent, ProjectSlug::new("sample-game").unwrap(), source);

    let prepared = prepare_desktop_2d_project(request).unwrap();
    let plan = prepared.plan();

    assert_eq!(
        plan.directories(),
        &[
            ProjectDirectory::Assets,
            ProjectDirectory::Prefabs,
            ProjectDirectory::Scenes,
            ProjectDirectory::Source,
            ProjectDirectory::SourceBinaries,
        ]
    );
    assert_eq!(
        plan.members()
            .iter()
            .map(|member| member.member())
            .collect::<Vec<_>>(),
        vec![
            ProjectMember::CargoManifest,
            ProjectMember::ProjectManifest,
            ProjectMember::AuthorGuide,
            ProjectMember::LibrarySource,
            ProjectMember::HeadlessSource,
            ProjectMember::DesktopSource,
            ProjectMember::StartupScene,
        ]
    );

    let cargo = plan.member(ProjectMember::CargoManifest).unwrap();
    let cargo = std::str::from_utf8(cargo.bytes()).unwrap();
    let cargo_toml = toml::from_str::<toml::Value>(cargo).unwrap();
    assert_eq!(cargo_toml["workspace"]["resolver"].as_str(), Some("3"));
    assert_eq!(cargo_toml["lib"]["name"].as_str(), Some("nara_game"));
    let binaries = cargo_toml["bin"].as_array().unwrap();
    assert_eq!(binaries.len(), 2);
    assert_eq!(binaries[0]["name"].as_str(), Some("headless"));
    assert_eq!(binaries[1]["name"].as_str(), Some("desktop"));
    assert_eq!(
        cargo_toml["dependencies"]["nara"]["git"].as_str(),
        Some("https://github.com/nara-engine/nara.git")
    );
    assert_eq!(
        cargo_toml["dependencies"]["nara"]["rev"].as_str(),
        Some(GIT_REVISION)
    );
    assert!(!cargo.contains("{{"));
    assert!(!cargo.contains("reference-game"));

    let manifest = plan.member(ProjectMember::ProjectManifest).unwrap();
    let load = nara::project::ProjectManifest::parse_toml_bytes(manifest.bytes());
    assert!(!load.has_errors());
    let manifest = load.manifest.unwrap();
    let headless = manifest.resolve_profile(None).unwrap();
    let desktop = manifest.resolve_profile(Some("desktop")).unwrap();
    assert!(
        headless
            .requested_capabilities
            .contains(nara::project::ProductCapability::Runtime2d)
    );
    assert!(
        desktop
            .requested_capabilities
            .contains(nara::project::ProductCapability::DesktopWinit)
    );
    assert!(
        desktop
            .requested_capabilities
            .contains(nara::project::ProductCapability::RenderWgpu)
    );

    assert!(!root.path().join("sample-game").exists());
}

#[test]
fn project_slug_rejects_unsafe_or_ambiguous_names() {
    for invalid in [
        "",
        "SampleGame",
        "sample_game",
        "-sample",
        "sample-",
        "sample--game",
        ".",
        "..",
        "con",
        "sample/game",
        "sample game",
    ] {
        assert!(ProjectSlug::new(invalid).is_err(), "accepted {invalid:?}");
    }

    assert!(ProjectSlug::new("sample-game-2").is_ok());
}

#[test]
fn immutable_git_source_rejects_mutable_or_unsafe_coordinates() {
    assert!(
        NaraDependencySource::immutable_git("https://github.com/nara-engine/nara.git", "main")
            .is_err()
    );
    assert!(NaraDependencySource::immutable_git("file:///tmp/nara", GIT_REVISION).is_err());
    assert!(
        NaraDependencySource::immutable_git(
            "https://github.com/nara-engine/nara.git\"",
            GIT_REVISION
        )
        .is_err()
    );
}

#[test]
fn local_source_derives_the_cargo_path_from_the_authorized_parent_relation() {
    let checkout = TestRoot::new("local-checkout");
    fs::create_dir(checkout.path().join("projects")).unwrap();
    let checkout_capability = checkout.writable_capability();
    let parent = checkout_capability
        .open_directory(&RelativePath::new("projects").unwrap())
        .unwrap();
    let source = NaraDependencySource::local(
        checkout_capability,
        LocalDependencyParent::Descendant(RelativePath::new("projects").unwrap()),
    );
    let request =
        ProjectCreationRequest::new(parent, ProjectSlug::new("local-game").unwrap(), source);

    let prepared = prepare_desktop_2d_project(request).unwrap();
    let cargo = prepared
        .plan()
        .member(ProjectMember::CargoManifest)
        .unwrap();
    let cargo = std::str::from_utf8(cargo.bytes()).unwrap();
    let cargo_toml = toml::from_str::<toml::Value>(cargo).unwrap();
    assert_eq!(
        cargo_toml["dependencies"]["nara"]["path"].as_str(),
        Some("../..")
    );
    assert!(!cargo.contains(checkout.path().to_string_lossy().as_ref()));
}

#[test]
fn local_source_rejects_a_parent_outside_the_checkout_relation() {
    let checkout = TestRoot::new("mismatched-checkout");
    let destination = TestRoot::new("mismatched-parent");
    let source = NaraDependencySource::local(
        checkout.writable_capability(),
        LocalDependencyParent::CheckoutRoot,
    );
    let request = ProjectCreationRequest::new(
        destination.writable_capability(),
        ProjectSlug::new("outside-game").unwrap(),
        source,
    );

    let error = prepare_desktop_2d_project(request).unwrap_err();
    assert_eq!(
        error.kind(),
        ProjectCreationPlanErrorKind::LocalParentMismatch
    );
    assert!(!destination.path().join("outside-game").exists());
}

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nara-project-workflow-{label}-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn writable_capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            open_directory(&self.path),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(windows)]
fn open_directory(path: &Path) -> File {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap()
}

#[cfg(unix)]
fn open_directory(path: &Path) -> File {
    File::open(path).unwrap()
}
