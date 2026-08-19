use std::{
    env,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, FileCapability, FsError, HostCapabilityOptions,
        PathValidationError, RelativePath, TrustMode,
    },
    tooling::{
        EditorPlayState, EditorProjectContentView, EditorRuntimeAvailability,
        EditorRuntimeUnavailableReason,
    },
};
use thiserror::Error;
use winit::window::WindowId;

use crate::{EditorApplication, EditorApplicationState, EditorLaunchOptions};

use super::EditorRunError;

const GENERIC_OPEN_CANARY_ENV: &str = "NARA_EDITOR_GENERIC_OPEN_CANARY";
const GENERIC_OPEN_CANARY_DEADLINE: Duration = Duration::from_secs(35);
const PROJECT_MANIFEST_PATH: &str = "nara.toml";
const AUTHORING_BUNDLE_PATH: &str = "catalogs/authoring.bundle.json";
const STARTUP_SCENE_PATH: &str = "worlds/entry.scene.json";
const TEMP_ROOT_PREFIX: &str = "nara-editor-generic-open-canary-";
const SOURCE_MEMBER_BYTE_LIMIT: u64 = 16 * 1024 * 1024;

pub(super) fn requested() -> bool {
    env::var_os(GENERIC_OPEN_CANARY_ENV).as_deref() == Some(OsStr::new("1"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorGenericOpenCanaryStage {
    ProjectManager,
    Recovery,
    AuthoringWorkspace,
    Complete,
}

pub(super) struct EditorGenericOpenCanaryHost {
    guarded_project: GuardedProject,
    stage: EditorGenericOpenCanaryStage,
    deadline: Instant,
    window_id: Option<WindowId>,
}

impl EditorGenericOpenCanaryHost {
    pub(super) fn new(
        options: EditorLaunchOptions,
    ) -> Result<(EditorApplication, Self), EditorRunError> {
        let source_root = options.project_root().ok_or_else(|| {
            EditorRunError::usage(format!(
                "{GENERIC_OPEN_CANARY_ENV}=1 requires one source project root"
            ))
        })?;
        let guarded_project = GuardedProject::prepare(source_root).map_err(canary_prepare_error)?;
        let launch = EditorLaunchOptions::new(guarded_project.root())
            .with_optional_profile(options.profile().map(str::to_owned));
        Ok((
            EditorApplication::new(launch),
            Self {
                guarded_project,
                stage: EditorGenericOpenCanaryStage::ProjectManager,
                deadline: Instant::now() + GENERIC_OPEN_CANARY_DEADLINE,
                window_id: None,
            },
        ))
    }

    pub(super) fn advance_project_shell(
        &mut self,
        rendered_state: EditorApplicationState,
        application: &mut EditorApplication,
        window_id: WindowId,
    ) -> Result<bool, EditorRunError> {
        self.require_before_deadline(application.state())?;
        self.observe_window(window_id, application.state())?;
        match self.stage {
            EditorGenericOpenCanaryStage::ProjectManager => {
                require_rendered_state(
                    rendered_state,
                    EditorApplicationState::ProjectManager,
                    self.stage,
                )?;
                self.require_before_deadline(application.state())?;
                let update = application.native_shell_ready();
                if !update.actions_observed()
                    || application.state() != EditorApplicationState::OpeningInitial
                {
                    return Err(canary_state_error(
                        self.stage,
                        application.state(),
                        "the initial project request was not armed after Project Manager presentation",
                    ));
                }
                self.require_before_deadline(application.state())?;
                self.stage = EditorGenericOpenCanaryStage::Recovery;
                Ok(true)
            }
            EditorGenericOpenCanaryStage::Recovery => match rendered_state {
                EditorApplicationState::OpeningInitial => Ok(false),
                EditorApplicationState::Recovery => {
                    self.guarded_project
                        .restore_authoring_bundle()
                        .map_err(canary_io_error)?;
                    self.require_before_deadline(application.state())?;
                    if !application.retry_project_open()
                        || application.state() != EditorApplicationState::OpeningInitial
                    {
                        return Err(canary_state_error(
                            self.stage,
                            application.state(),
                            "Recovery did not accept the guarded Retry",
                        ));
                    }
                    self.require_before_deadline(application.state())?;
                    self.stage = EditorGenericOpenCanaryStage::AuthoringWorkspace;
                    Ok(true)
                }
                state => Err(canary_state_error(
                    self.stage,
                    state,
                    "the missing authoring bundle did not reach Recovery",
                )),
            },
            EditorGenericOpenCanaryStage::AuthoringWorkspace => match rendered_state {
                EditorApplicationState::OpeningInitial => Ok(false),
                EditorApplicationState::Recovery => Err(canary_state_error(
                    self.stage,
                    rendered_state,
                    "the repaired authoring bundle was rejected",
                )),
                state => Err(canary_state_error(
                    self.stage,
                    state,
                    "the repaired project left the bounded initial-open trajectory",
                )),
            },
            EditorGenericOpenCanaryStage::Complete => Ok(false),
        }
    }

    pub(super) fn advance_workspace(
        &mut self,
        rendered_state: EditorApplicationState,
        application: &EditorApplication,
        window_id: WindowId,
    ) -> Result<bool, EditorRunError> {
        self.require_before_deadline(application.state())?;
        self.observe_window(window_id, application.state())?;
        if self.stage != EditorGenericOpenCanaryStage::AuthoringWorkspace {
            return Err(canary_state_error(
                self.stage,
                rendered_state,
                "a workspace was published before the guarded repair completed",
            ));
        }
        require_rendered_state(
            rendered_state,
            EditorApplicationState::AuthoringDegraded,
            self.stage,
        )?;
        require_workspace_contract(application, self.stage)?;
        self.require_before_deadline(application.state())?;
        self.stage = EditorGenericOpenCanaryStage::Complete;
        Ok(true)
    }

    pub(super) const fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn timeout_error(
        &self,
        application_state: EditorApplicationState,
    ) -> EditorRunError {
        EditorRunError::Platform(format!(
            "the generic Open/Recovery canary timed out in {:?}; application state={application_state:?}",
            self.stage
        ))
    }

    pub(super) fn finish(self) -> Result<(), EditorRunError> {
        if self.stage != EditorGenericOpenCanaryStage::Complete {
            return Err(EditorRunError::Platform(format!(
                "the native Editor exited before the generic Open/Recovery canary completed ({:?})",
                self.stage
            )));
        }
        println!(
            "{{\"kind\":\"nara_editor_generic_open_canary\",\"version\":1,\"window\":\"single\",\"project_manager\":\"presented\",\"recovery\":\"presented\",\"workspace\":\"authoring_degraded\",\"runtime\":\"external_host_unavailable\",\"project_content\":\"not_installed\",\"play\":\"empty\",\"result\":\"ok\"}}"
        );
        Ok(())
    }

    fn require_before_deadline(
        &self,
        application_state: EditorApplicationState,
    ) -> Result<(), EditorRunError> {
        if Instant::now() < self.deadline {
            return Ok(());
        }
        Err(self.timeout_error(application_state))
    }

    fn observe_window(
        &mut self,
        window_id: WindowId,
        application_state: EditorApplicationState,
    ) -> Result<(), EditorRunError> {
        match self.window_id {
            None => {
                self.window_id = Some(window_id);
                Ok(())
            }
            Some(expected) if expected == window_id => Ok(()),
            Some(expected) => Err(EditorRunError::Platform(format!(
                "the generic Open/Recovery canary changed native windows; canary stage={:?}, application state={application_state:?}, expected window={expected:?}, presented window={window_id:?}",
                self.stage
            ))),
        }
    }
}

fn require_workspace_contract(
    application: &EditorApplication,
    stage: EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    let state = application.state();
    let frame = application.current_frame().ok_or_else(|| {
        canary_state_error(
            stage,
            state,
            "the repaired workspace has no published Editor frame",
        )
    })?;
    let runtime = frame.runtime_availability();
    let project_content = frame.project_content();
    let play = frame.play();
    let pending_game_start = application.pending_game_start_request().is_some();
    let active_game_start = application.active_game_start_request().is_some();
    if runtime
        == EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
        )
        && project_content == EditorProjectContentView::NotInstalled
        && play.state() == EditorPlayState::Empty
        && play.generation().is_none()
        && !pending_game_start
        && !active_game_start
    {
        return Ok(());
    }
    Err(EditorRunError::Platform(format!(
        "the generic Open/Recovery canary workspace violated the ordinary execution boundary; canary stage={stage:?}, application state={state:?}, runtime={runtime:?}, project content={project_content:?}, play state={:?}, play generation={:?}, pending game start={pending_game_start}, active game start={active_game_start}",
        play.state(),
        play.generation(),
    )))
}

fn require_rendered_state(
    actual: EditorApplicationState,
    expected: EditorApplicationState,
    stage: EditorGenericOpenCanaryStage,
) -> Result<(), EditorRunError> {
    if actual == expected {
        return Ok(());
    }
    Err(canary_state_error(
        stage,
        actual,
        "the native window presented an unexpected product surface",
    ))
}

fn canary_state_error(
    stage: EditorGenericOpenCanaryStage,
    application_state: EditorApplicationState,
    summary: &str,
) -> EditorRunError {
    EditorRunError::Platform(format!(
        "{summary}; canary stage={stage:?}, application state={application_state:?}"
    ))
}

fn canary_prepare_error(error: GuardedProjectError) -> EditorRunError {
    EditorRunError::Platform(format!(
        "the generic Open/Recovery canary could not prepare its guarded project: {error}"
    ))
}

fn canary_io_error(error: io::Error) -> EditorRunError {
    EditorRunError::Platform(format!(
        "the generic Open/Recovery canary could not repair its guarded project: {error}"
    ))
}

#[derive(Debug, Error)]
enum GuardedProjectError {
    #[error("the source project root could not be inspected")]
    SourceRootInspect(#[source] io::Error),
    #[error("the source project root must be one real directory: {0}")]
    SourceRootInvalid(PathBuf),
    #[error("the source project root could not be opened")]
    SourceRootOpen(#[source] io::Error),
    #[error("the source project root did not satisfy untrusted capability policy")]
    SourceRootCapability(#[source] FsError),
    #[error("the fixed canary source member path is invalid: {path}")]
    SourceMemberPath {
        path: &'static str,
        #[source]
        source: PathValidationError,
    },
    #[error("the canary source must provide one safe regular member: {path}")]
    SourceMemberOpen {
        path: &'static str,
        #[source]
        source: FsError,
    },
    #[error("the canary source member could not be read within its byte limit: {path}")]
    SourceMemberRead {
        path: &'static str,
        #[source]
        source: FsError,
    },
    #[error("the guarded destination could not be prepared")]
    Destination(#[from] io::Error),
}

struct GuardedProject {
    root: PathBuf,
    temp_parent: PathBuf,
    authoring_bundle: Vec<u8>,
}

impl GuardedProject {
    fn prepare(source_root: &Path) -> Result<Self, GuardedProjectError> {
        let source_metadata =
            fs::symlink_metadata(source_root).map_err(GuardedProjectError::SourceRootInspect)?;
        if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
            return Err(GuardedProjectError::SourceRootInvalid(
                source_root.to_path_buf(),
            ));
        }
        let source_root = open_source_root(source_root)?;
        let authoring_bundle = read_source_member(&source_root, AUTHORING_BUNDLE_PATH)?;
        let temp_parent = env::temp_dir();
        let root = create_temp_root(&temp_parent)?;
        if let Err(error) = copy_project_without_bundle(&source_root, &root) {
            let _ = fs::remove_dir_all(&root);
            return Err(error);
        }
        Ok(Self {
            root,
            temp_parent,
            authoring_bundle,
        })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn restore_authoring_bundle(&self) -> io::Result<()> {
        let destination = self.root.join(AUTHORING_BUNDLE_PATH);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&destination)?;
        let result = file
            .write_all(&self.authoring_bundle)
            .and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = result {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for GuardedProject {
    fn drop(&mut self) {
        let safe_name = self
            .root
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with(TEMP_ROOT_PREFIX));
        if self.root.parent() == Some(self.temp_parent.as_path()) && safe_name {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn create_temp_root(temp_parent: &Path) -> io::Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..32_u8 {
        let candidate = temp_parent.join(format!(
            "{TEMP_ROOT_PREFIX}{}-{stamp}-{attempt}",
            process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique guarded canary directory",
    ))
}

fn copy_project_without_bundle(
    source_root: &DirectoryCapability,
    destination_root: &Path,
) -> Result<(), GuardedProjectError> {
    fs::create_dir(destination_root.join("catalogs"))?;
    fs::create_dir(destination_root.join("worlds"))?;
    for relative in [PROJECT_MANIFEST_PATH, STARTUP_SCENE_PATH] {
        let bytes = read_source_member(source_root, relative)?;
        fs::write(destination_root.join(relative), bytes)?;
    }
    Ok(())
}

fn open_source_root(path: &Path) -> Result<DirectoryCapability, GuardedProjectError> {
    let file = open_directory_handle(path).map_err(GuardedProjectError::SourceRootOpen)?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
    )
    .map_err(GuardedProjectError::SourceRootCapability)
}

fn read_source_member(
    root: &DirectoryCapability,
    path: &'static str,
) -> Result<Vec<u8>, GuardedProjectError> {
    let relative = RelativePath::new(path)
        .map_err(|source| GuardedProjectError::SourceMemberPath { path, source })?;
    let file: FileCapability = root
        .open_file(&relative)
        .map_err(|source| GuardedProjectError::SourceMemberOpen { path, source })?;
    file.read_to_end_bounded(SOURCE_MEMBER_BYTE_LIMIT)
        .map_err(|source| GuardedProjectError::SourceMemberRead { path, source })
}

#[cfg(windows)]
fn open_directory_handle(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_directory_handle(path: &Path) -> io::Result<File> {
    File::open(path)
}
