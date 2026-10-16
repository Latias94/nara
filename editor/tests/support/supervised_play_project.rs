use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    project_workflow::{
        LocalDependencyRelation, NaraDependencySource, ProjectCreationOutcome,
        ProjectCreationRequest, ProjectSlug, prepare_desktop_2d_project,
    },
};

const PROJECT_SLUG: &str = "clean-room-game";
const TEMP_ROOT_PREFIX: &str = "nara-sp-";

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(1);

pub struct CleanRoomSupervisedPlayProject {
    allocation_root: PathBuf,
    target_root: PathBuf,
    project_root: PathBuf,
}

impl CleanRoomSupervisedPlayProject {
    pub fn new() -> Self {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the Editor package has a workspace parent")
            .canonicalize()
            .expect("the workspace checkout is canonical");
        let target_root = checkout.join("target");
        fs::create_dir_all(&target_root).expect("the workspace target directory is available");
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let allocation_name = format!("{TEMP_ROOT_PREFIX}{}-{sequence:016x}", std::process::id());
        let allocation_root = target_root.join(&allocation_name);
        let project_root = allocation_root.join(PROJECT_SLUG);
        fs::create_dir(&allocation_root).expect("the clean-room allocation is fresh");

        let project = Self {
            allocation_root,
            target_root,
            project_root,
        };
        let parent_relation = RelativePath::new(format!("target/{allocation_name}"))
            .expect("the generated clean-room relation is valid");
        let checkout_capability = writable_directory(&checkout);
        let publication_parent = checkout_capability
            .open_directory(&parent_relation)
            .expect("the clean-room publication parent is authorized");
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            publication_parent,
            ProjectSlug::new(PROJECT_SLUG).expect("the clean-room slug is valid"),
            NaraDependencySource::local(LocalDependencyRelation::new(
                checkout_capability,
                None,
                Some(parent_relation),
            )),
        ))
        .expect("the canonical desktop starter is valid");
        let ProjectCreationOutcome::Created(receipt) = prepared.publish() else {
            panic!("the clean-room starter project was not published and admitted");
        };
        assert_eq!(receipt.plan().slug().as_str(), PROJECT_SLUG);
        drop(receipt);

        let lock_status = Command::new(env!("CARGO"))
            .current_dir(&project.project_root)
            .args(["generate-lockfile", "--offline"])
            .env("CARGO_BUILD_JOBS", "1")
            .env("CARGO_TERM_COLOR", "never")
            .status()
            .expect("Cargo must start for the clean-room lockfile");
        assert!(
            lock_status.success(),
            "the clean-room lockfile must resolve from the already available offline registry"
        );
        assert!(project.project_root.join("Cargo.lock").is_file());
        project
    }

    pub fn root(&self) -> &Path {
        &self.project_root
    }
}

impl Drop for CleanRoomSupervisedPlayProject {
    fn drop(&mut self) {
        assert_eq!(
            self.allocation_root.parent(),
            Some(self.target_root.as_path())
        );
        assert!(
            self.allocation_root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(TEMP_ROOT_PREFIX))
        );
        if self.allocation_root.exists() {
            fs::remove_dir_all(&self.allocation_root)
                .expect("the clean-room project and its build products must retire");
        }
    }
}

fn writable_directory(path: &Path) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(path, true),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .expect("the clean-room checkout capability is valid")
}

#[cfg(windows)]
fn host_directory(path: &Path, writable: bool) -> File {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .write(writable)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .expect("the clean-room host directory is available")
}

#[cfg(unix)]
fn host_directory(path: &Path, _writable: bool) -> File {
    File::open(path).expect("the clean-room host directory is available")
}
