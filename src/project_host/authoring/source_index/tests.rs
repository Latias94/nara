#![cfg(windows)]

use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};

use super::*;
use crate::project_host::authoring::AuthoringProjectAdmissionHost;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const BUNDLE_PATH: &str = "schema/authoring.bundle.json";

#[test]
fn mixed_source_tree_publishes_once_in_lexical_order_with_safe_rejections() {
    let project = TestSourceProject::new("mixed", true);
    project.write("assets/textures/safe.png", b"safe image");
    project.write("assets/textures/safe.png.meta", b"metadata is inspect-only");
    project.write("assets/textures/linked.bin", b"linked");
    fs::hard_link(
        project.path("assets/textures/linked.bin"),
        project.path("assets/textures/linked-copy.bin"),
    )
    .unwrap();
    project.write("scenes/external.scene.json", b"{}");
    project.write("scenes/readme.txt", b"not a document");
    project.write("prefabs/external.prefab.json", b"{}");
    project.write("scripts/player.rs", b"fn player() {}");
    project.write("scripts/nested/helpers.rs", b"fn helper() {}");

    let schema = project.schema_receipt();
    let mut state = ProjectSourceIndexState::with_limits(
        &schema,
        ProjectSourceIndexLimits {
            max_entries_per_update: 2,
            max_directories_per_update: 1,
            ..ProjectSourceIndexLimits::default()
        },
    );
    assert!(state.request_refresh());
    state.advance();
    assert_eq!(state.kind(), ProjectSourceIndexStateKind::Scanning);
    assert!(state.snapshot().is_none());

    drive_to_terminal(&mut state);

    assert_eq!(
        state.kind(),
        ProjectSourceIndexStateKind::ReadyWithDiagnostics
    );
    let snapshot = state.snapshot().unwrap();
    assert_eq!(snapshot.generation().get(), 1);
    assert_eq!(
        snapshot.completion(),
        ProjectSourceIndexCompletion::ReadyWithDiagnostics
    );
    let paths = snapshot
        .entries()
        .iter()
        .map(ProjectSourceIndexEntry::relative_path)
        .collect::<Vec<_>>();
    assert!(paths.windows(2).all(|pair| pair[0] < pair[1]));
    assert_entry(
        &snapshot,
        PROJECT_MANIFEST_PATH,
        ProjectSourceKind::ProjectManifest,
    );
    assert_entry(
        &snapshot,
        BUNDLE_PATH,
        ProjectSourceKind::AuthoringSchemaBundle,
    );
    assert_entry(
        &snapshot,
        "assets/textures/safe.png",
        ProjectSourceKind::AssetSource,
    );
    assert_entry(
        &snapshot,
        "assets/textures/safe.png.meta",
        ProjectSourceKind::AssetMeta,
    );
    assert_entry(
        &snapshot,
        "scenes/external.scene.json",
        ProjectSourceKind::Scene,
    );
    assert_eq!(
        snapshot
            .entry("scenes/external.scene.json")
            .unwrap()
            .open_capability(),
        ProjectSourceOpenCapability::Scene
    );
    assert_entry(
        &snapshot,
        "prefabs/external.prefab.json",
        ProjectSourceKind::Prefab,
    );
    assert_entry(&snapshot, "scripts/player.rs", ProjectSourceKind::Script);
    assert_entry(&snapshot, "scenes/readme.txt", ProjectSourceKind::Unknown);
    assert_entry(
        &snapshot,
        "assets/textures/linked.bin",
        ProjectSourceKind::Unsupported,
    );
    assert!(snapshot.diagnostics().iter().all(|diagnostic| {
        diagnostic.code() == ProjectSourceIndexDiagnosticCode::EntryUnsupported
            && diagnostic
                .relative_source()
                .is_some_and(|path| !path.contains(project.root.to_string_lossy().as_ref()))
    }));
}

#[test]
fn refresh_budget_failure_retains_the_last_good_generation() {
    let project = TestSourceProject::new("budget", false);
    project.write("assets/safe.bin", b"safe");
    project.write("scenes/startup.scene.json", b"{}");
    project.write("prefabs/base.prefab.json", b"{}");
    let schema = project.schema_receipt();
    let mut state = ProjectSourceIndexState::with_limits(
        &schema,
        ProjectSourceIndexLimits {
            max_entries: 48,
            max_entries_per_update: 8,
            max_directories_per_update: 2,
            ..ProjectSourceIndexLimits::default()
        },
    );
    assert!(state.request_refresh());
    drive_to_terminal(&mut state);
    assert_eq!(
        state.kind(),
        ProjectSourceIndexStateKind::ReadyWithDiagnostics
    );
    let first = state.snapshot().unwrap();
    assert_eq!(first.generation().get(), 1);
    assert!(first.effective_roots().iter().any(|root| {
        root.relative_path() == "scripts"
            && root.availability() == ProjectSourceRootAvailability::Missing
    }));

    for index in 0..64 {
        project.write(&format!("assets/generated-{index:03}.bin"), b"generated");
    }
    assert!(state.request_refresh());
    drive_to_terminal(&mut state);

    assert_eq!(state.kind(), ProjectSourceIndexStateKind::Unavailable);
    let retained = state.snapshot().unwrap();
    assert_eq!(retained.generation().get(), 1);
    assert!(Arc::ptr_eq(&first, &retained));
    assert_eq!(
        state.failure().unwrap().diagnostic().code(),
        ProjectSourceIndexDiagnosticCode::EntryBudgetExceeded
    );
}

#[test]
fn final_lineage_revalidation_rejects_directory_replacement() {
    let project = TestSourceProject::new("lineage", true);
    project.write("assets/stable.bin", b"stable");
    project.write("scenes/startup.scene.json", b"{}");
    project.write("prefabs/base.prefab.json", b"{}");
    project.write("scripts/player.rs", b"fn player() {}");
    let schema = project.schema_receipt();
    let mut state = ProjectSourceIndexState::with_limits(
        &schema,
        ProjectSourceIndexLimits {
            max_entries_per_update: 8,
            max_directories_per_update: 1,
            ..ProjectSourceIndexLimits::default()
        },
    );
    assert!(state.request_refresh());
    drive_to_terminal(&mut state);
    let first = state.snapshot().unwrap();
    assert_eq!(first.generation().get(), 1);

    assert!(state.request_refresh());
    while state.has_pending_work() && !state.is_revalidating() {
        state.advance();
    }
    assert!(state.is_revalidating());
    fs::rename(project.path("assets"), project.path("assets-old")).unwrap();
    fs::create_dir(project.path("assets")).unwrap();
    drive_to_terminal(&mut state);

    assert_eq!(state.kind(), ProjectSourceIndexStateKind::Unavailable);
    assert!(Arc::ptr_eq(&first, &state.snapshot().unwrap()));
    assert_eq!(
        state.failure().unwrap().diagnostic().code(),
        ProjectSourceIndexDiagnosticCode::DirectoryLineageChanged
    );
}

fn assert_entry(snapshot: &ProjectSourceIndexSnapshot, path: &str, expected: ProjectSourceKind) {
    assert_eq!(
        snapshot.entry(path).map(ProjectSourceIndexEntry::kind),
        Some(expected)
    );
}

fn drive_to_terminal(state: &mut ProjectSourceIndexState) {
    for _ in 0..1_024 {
        if !state.has_pending_work() {
            return;
        }
        state.advance();
    }
    panic!("source index did not reach a terminal state");
}

struct TestSourceProject {
    root: PathBuf,
}

impl TestSourceProject {
    fn new(name: &str, create_scripts: bool) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_source_index_{}_{}_{}",
            std::process::id(),
            sequence,
            name
        ));
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        if create_scripts {
            fs::create_dir_all(root.join("scripts")).unwrap();
        }
        let manifest = format!(
            r#"schema_version = 1

[project]
name = "{name}"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
scripts = "scripts"

[authoring]
schema_bundle = "{BUNDLE_PATH}"
"#
        );
        fs::write(root.join(PROJECT_MANIFEST_PATH), manifest).unwrap();
        let bundle =
            AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
        fs::write(root.join(BUNDLE_PATH), bundle.to_json_string().unwrap()).unwrap();
        Self { root }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root
            .join(relative.replace('/', std::path::MAIN_SEPARATOR_STR))
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    fn schema_receipt(&self) -> AuthoringSchemaReceipt {
        AuthoringProjectAdmissionHost::default()
            .prepare(self.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish()
    }

    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
        )
        .unwrap()
    }
}

impl Drop for TestSourceProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn host_directory(path: &Path) -> io::Result<File> {
    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}
