#![cfg(feature = "project-workflow")]

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, FsError, HostCapabilityOptions,
        PublicationAtomicity, PublicationIdentityEvidence, PublicationSourceBinding, RelativePath,
        TrustMode,
    },
    project_workflow::{
        LocalDependencyParent, NaraDependencySource, ProjectCreationOutcome,
        ProjectCreationPlanErrorKind, ProjectCreationRejectionKind, ProjectCreationRequest,
        ProjectDirectory, ProjectMember, ProjectSlug, prepare_desktop_2d_project,
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
            ProjectDirectory::Schema,
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
            ProjectMember::AuthoringSchemaBundle,
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
    let guide = plan.member(ProjectMember::AuthorGuide).unwrap();
    let guide = std::str::from_utf8(guide.bytes()).unwrap();
    assert!(guide.starts_with("# sample-game author guide\n"));
    assert!(guide.contains("Starter: `desktop-2d/v1`"));
    assert!(guide.contains(GIT_REVISION));
    assert!(guide.contains(&format!(
        "nara new desktop-2d/v1 sample-game --parent . --git https://github.com/nara-engine/nara.git --rev {GIT_REVISION}"
    )));
    assert!(guide.contains("nara check ."));
    assert!(guide.contains("nara export ."));
    assert!(guide.contains("nara repair startup-scene ."));
    assert_eq!(
        plan.dependency().immutable_git_repository_digest(),
        Some(ContentDigest::of_bytes(
            b"https://github.com/nara-engine/nara.git"
        ))
    );
    assert_eq!(
        plan.dependency().immutable_git_revision(),
        Some(GIT_REVISION)
    );

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
    assert_eq!(
        desktop.execution.contract.as_ref(),
        Some(&nara::project::ProjectExecutionContract::DesktopSupervisedPlayV1)
    );
    assert_eq!(
        manifest.authoring.schema_bundle.as_deref(),
        Some("schema/authoring.bundle.json")
    );
    assert_eq!(
        headless
            .authoring_schema_bundle
            .as_ref()
            .map(|path| path.as_str()),
        Some("schema/authoring.bundle.json")
    );
    assert_eq!(
        desktop.authoring_schema_bundle,
        headless.authoring_schema_bundle
    );

    let bundle_member = plan.member(ProjectMember::AuthoringSchemaBundle).unwrap();
    let bundle =
        nara::reflect::AuthoringSchemaBundle::from_json_bytes(bundle_member.bytes()).unwrap();
    assert_eq!(
        bundle.to_json_string().unwrap().as_bytes(),
        bundle_member.bytes()
    );
    let providers = [
        nara::scene::SCENE_COMPONENTS_SCHEMA_PROVIDER,
        nara::transform::TRANSFORM_SCHEMA_PROVIDER,
        nara::render::RENDER_SCHEMA_PROVIDER,
        nara::sprite::SPRITE_SCHEMA_PROVIDER,
        nara::tilemap::TILEMAP_SCHEMA_PROVIDER,
    ];
    let mut expected_owner_ids = providers
        .iter()
        .map(|provider| provider.owner().as_str())
        .collect::<Vec<_>>();
    expected_owner_ids.sort_unstable();
    assert_eq!(
        bundle
            .owners()
            .iter()
            .map(|owner| owner.owner_id().as_str())
            .collect::<Vec<_>>(),
        expected_owner_ids
    );
    for provider in providers {
        let resolved = nara::reflect::__private::resolve_schema_provider(provider).unwrap();
        let owner_id = nara::reflect::PersistentComponentSchemaOwnerId::try_new(
            resolved.current().owner().as_str(),
        )
        .unwrap();
        let bundled = bundle.owner(&owner_id).unwrap();
        assert_eq!(bundled.current(), resolved.current().catalog());
        assert_eq!(
            bundled.predecessor(),
            resolved
                .predecessor()
                .map(|predecessor| predecessor.catalog())
        );
    }

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
    for repository in [
        "https://user@example.com/nara.git",
        "https://user:password@example.com/nara.git",
        "https://example.com:443/nara.git?token=secret",
        "https://example.com:+443/nara.git",
        "https://example.com/nara;touch-pwned.git",
    ] {
        assert!(
            NaraDependencySource::immutable_git(repository, GIT_REVISION).is_err(),
            "accepted unsafe Git repository {repository:?}"
        );
    }
}

#[test]
fn local_source_derives_the_cargo_path_from_the_authorized_parent_relation() {
    let checkout = TestRoot::new("local-checkout");
    fs::create_dir(checkout.path().join("projects")).unwrap();
    let checkout_capability = checkout.writable_capability();
    let checkout_identity = checkout_capability.identity();
    let parent = checkout_capability
        .open_directory(&RelativePath::new("projects").unwrap())
        .unwrap();
    let parent_identity = parent.identity();
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
    assert_eq!(
        prepared.plan().dependency().local_checkout_identity(),
        Some(checkout_identity)
    );
    assert_eq!(
        prepared
            .plan()
            .dependency()
            .local_destination_parent_identity(),
        Some(parent_identity)
    );
    assert_eq!(
        prepared.plan().dependency().local_descendant_depth(),
        Some(1)
    );
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

#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn local_source_revalidates_the_checkout_relation_before_staging() {
    let checkout = TestRoot::new("relation-drift-checkout");
    fs::create_dir(checkout.path().join("projects")).unwrap();
    let checkout_capability = checkout.writable_capability();
    let publication_parent = checkout_capability
        .open_directory(&RelativePath::new("projects").unwrap())
        .unwrap();
    let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
        publication_parent,
        ProjectSlug::new("drifted-game").unwrap(),
        NaraDependencySource::local(
            checkout_capability,
            LocalDependencyParent::Descendant(RelativePath::new("projects").unwrap()),
        ),
    ))
    .unwrap();
    let outside = TestRoot::new("relation-drift-outside");
    let moved_parent = outside.path().join("moved-projects");
    fs::rename(checkout.path().join("projects"), &moved_parent).unwrap();

    let outcome = prepared.publish();
    let ProjectCreationOutcome::Rejected(rejection) = outcome else {
        panic!("expected dependency-relation rejection: {outcome:?}");
    };

    assert_eq!(
        rejection.kind(),
        ProjectCreationRejectionKind::DependencyRelation
    );
    assert!(!moved_parent.join("drifted-game").exists());
    assert!(fs::read_dir(&moved_parent).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".nara-stage-")
    }));
}

#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn published_project_is_reopened_and_admitted_through_ordinary_authority() {
    let root = TestRoot::new("published-and-admitted");
    let source = NaraDependencySource::immutable_git(
        "https://github.com/nara-engine/nara.git",
        GIT_REVISION,
    )
    .unwrap();
    let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
        root.writable_capability(),
        ProjectSlug::new("published-game").unwrap(),
        source,
    ))
    .unwrap();

    let outcome = prepared.publish();
    if publication_is_unavailable_and_clean(&outcome, root.path(), "published-game") {
        return;
    }
    let ProjectCreationOutcome::Created(receipt) = outcome else {
        panic!("expected an admitted creation receipt: {outcome:?}");
    };

    assert_eq!(
        receipt.publication().published_identity(),
        receipt.project_root_identity()
    );
    assert_eq!(
        receipt.publication().publication_atomicity(),
        PublicationAtomicity::AtomicNameSwitch
    );
    #[cfg(windows)]
    {
        assert_eq!(
            receipt.publication().source_binding(),
            PublicationSourceBinding::HandleBound
        );
        assert_eq!(
            receipt.publication().publication_identity_evidence(),
            PublicationIdentityEvidence::HandleBoundCandidate
        );
    }
    #[cfg(target_os = "linux")]
    {
        assert_eq!(
            receipt.publication().source_binding(),
            PublicationSourceBinding::NameBound
        );
        assert_eq!(
            receipt.publication().publication_identity_evidence(),
            PublicationIdentityEvidence::NameBoundCandidate
        );
    }
    assert_eq!(receipt.plan().starter().id(), "desktop-2d/v1");
    assert_eq!(receipt.plan().manifest_schema_version(), 1);
    assert_eq!(receipt.plan().scene_format_version(), 1);
    assert_eq!(
        receipt.plan().directories(),
        &[
            ProjectDirectory::Assets,
            ProjectDirectory::Prefabs,
            ProjectDirectory::Scenes,
            ProjectDirectory::Schema,
            ProjectDirectory::Source,
            ProjectDirectory::SourceBinaries,
        ]
    );
    assert_eq!(receipt.plan().members().len(), 8);

    let project_root = receipt.project_root();
    let manifest = project_root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let manifest_receipt = receipt
        .plan()
        .member(ProjectMember::ProjectManifest)
        .unwrap();
    manifest
        .verify_digest(
            manifest_receipt.digest(),
            manifest_receipt.digest().length(),
        )
        .unwrap();
    let authoring_bundle = project_root
        .open_file(&RelativePath::new("schema/authoring.bundle.json").unwrap())
        .unwrap();
    let authoring_bundle_receipt = receipt
        .plan()
        .member(ProjectMember::AuthoringSchemaBundle)
        .unwrap();
    authoring_bundle
        .verify_digest(
            authoring_bundle_receipt.digest(),
            authoring_bundle_receipt.digest().length(),
        )
        .unwrap();

    for path in [
        "Cargo.toml",
        "nara.toml",
        "schema/authoring.bundle.json",
        "README.md",
        "src/lib.rs",
        "src/bin/headless.rs",
        "src/bin/desktop.rs",
        "scenes/startup.scene.json",
    ] {
        assert!(root.path().join("published-game").join(path).is_file());
    }
}

#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn destination_collision_rejects_without_overwrite_or_staging_residue() {
    let root = TestRoot::new("destination-collision");
    let destination = root.path().join("existing-game");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep.txt"), b"existing").unwrap();
    let source = NaraDependencySource::immutable_git(
        "https://github.com/nara-engine/nara.git",
        GIT_REVISION,
    )
    .unwrap();
    let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
        root.writable_capability(),
        ProjectSlug::new("existing-game").unwrap(),
        source,
    ))
    .unwrap();

    let outcome = prepared.publish();
    let ProjectCreationOutcome::Rejected(rejection) = outcome else {
        panic!("expected a clean rejection: {outcome:?}");
    };

    assert_eq!(
        rejection.kind(),
        ProjectCreationRejectionKind::BeforeStaging
    );
    assert!(matches!(
        rejection.before_staging_error(),
        Some(FsError::TargetStateMismatch)
            | Some(FsError::Unsupported { .. })
            | Some(FsError::Unproven { .. })
    ));
    assert_eq!(rejection.plan().starter().id(), "desktop-2d/v1");
    assert_eq!(fs::read(destination.join("keep.txt")).unwrap(), b"existing");
    assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".nara-stage-")
    }));
}

fn publication_is_unavailable_and_clean(
    outcome: &ProjectCreationOutcome,
    parent: &Path,
    destination: &str,
) -> bool {
    let ProjectCreationOutcome::Rejected(rejection) = outcome else {
        return false;
    };
    if !matches!(
        rejection.before_staging_error(),
        Some(FsError::Unsupported { .. }) | Some(FsError::Unproven { .. })
    ) {
        return false;
    }
    assert!(!parent.join(destination).exists());
    assert!(fs::read_dir(parent).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".nara-stage-")
    }));
    true
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
