//! Game-owned finite process control over the shared native custody implementation.

use std::{
    ffi::OsStr,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape},
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, ExpectedTarget, FileCapability,
        RelativeComponent, RestrictiveCapabilityHandleLease, TemporaryFileFailure,
    },
    project_tooling::{
        cargo_build::CapturedCargoExecutable,
        host_fs,
        process::{
            ExactExecutableLaunchAuthority, ProcessLogLimits, SupervisedProcessCleanupReceipt,
            SupervisedProcessOwner, SupervisedProcessState, SupervisedProductEnvironmentPolicy,
            SupervisedProductProcessCommand,
        },
    },
};

use crate::resume::ResumeState;

use super::protocol::{
    Command, DesktopPresentation, DevelopmentPlacement, ExecutionState, ExecutionStatus,
    InputActivation, MAX_IDENTITY_BYTES, MAX_MESSAGE_BYTES, MAX_STEP_COMMANDS, Observation,
    PROTOCOL_VERSION, READY_FILE, REQUEST_FILE, Reply, Request, Response, SemanticCommand,
};
use super::timing::{self, DevelopmentTimingPhase, DevelopmentTimingSnapshot};

const STORE_PREFIX: &str = "nara-two-level-development-";
const STORE_MARKER: &str = "owner";
const MAX_DIRECTORIES: u64 = 64;
const MAX_REQUESTS: u64 = 512;
const CLEANUP_REQUESTS: u64 = 8;
const MAX_SETTLE_FRAMES: usize = 128;
const START_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const COOPERATIVE_CLOSE_TIMEOUT: Duration = Duration::from_secs(6);
const CLEANUP_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(10);
const MAX_LOG_RECORDS: usize = 256;
const MAX_LOG_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct Store {
    identity: Arc<StoreIdentity>,
    next: u64,
    cleaning: bool,
    closed: bool,
}

struct StoreIdentity {
    named_parent: PathBuf,
    parent: PathBuf,
    parent_directory: DirectoryCapability,
    root: PathBuf,
    directory: DirectoryCapability,
    marker: FileCapability,
    marker_digest: ContentDigest,
    session_prefix: String,
    directory_lease: Option<RestrictiveCapabilityHandleLease>,
    marker_lease: Option<RestrictiveCapabilityHandleLease>,
}

impl Store {
    pub(super) fn new(parent: &Path) -> Result<Self, String> {
        if !parent.is_absolute() {
            return Err("the development store requires an explicit absolute parent".into());
        }
        physical_directory(parent)?;
        let named_parent = parent.to_path_buf();
        let parent = fs::canonicalize(parent)
            .map_err(|error| format!("resolve development store parent: {error}"))?;
        let parent_directory = host_fs::readable_directory(&parent)
            .map_err(|error| format!("open development store parent: {error}"))?;
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let sequence = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "development store identity sequence is exhausted")?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("read development store creation time: {error}"))?
            .as_nanos();
        let session_prefix = format!("{}-{stamp:x}-{sequence:x}", std::process::id());
        let root = parent.join(format!("{STORE_PREFIX}{session_prefix}"));
        create_private_directory(&root)?;
        let result = (|| {
            let directory = host_fs::writable_directory(&root)
                .map_err(|error| format!("open new development store: {error}"))?;
            let marker_name = component(STORE_MARKER)?;
            let mut marker = directory
                .create_child_file(&marker_name)
                .map_err(|error| format!("create development store marker: {error}"))?;
            marker
                .writer()
                .map_err(|error| format!("open development store marker writer: {error}"))?
                .write_all(session_prefix.as_bytes())
                .map_err(|error| format!("write development store marker: {error}"))?;
            marker
                .sync()
                .map_err(|error| format!("sync development store marker: {error}"))?;
            drop(marker);
            drop(directory);
            let directory = host_fs::readable_directory(&root)
                .map_err(|error| format!("retain development store directory: {error}"))?;
            let marker = directory
                .open_child_file(&marker_name)
                .map_err(|error| format!("retain development store marker: {error}"))?;
            let directory_lease = directory
                .duplicate_restrictive_host_handle()
                .map_err(|error| format!("retain development store directory lease: {error}"))?;
            let marker_lease = marker
                .duplicate_restrictive_host_handle()
                .map_err(|error| format!("retain development store marker lease: {error}"))?;
            let identity = StoreIdentity {
                named_parent,
                parent,
                parent_directory,
                root: root.clone(),
                directory,
                marker,
                marker_digest: ContentDigest::of_bytes(session_prefix.as_bytes()),
                session_prefix,
                directory_lease: Some(directory_lease),
                marker_lease: Some(marker_lease),
            };
            identity.verify(false)?;
            Ok(Self {
                identity: Arc::new(identity),
                next: 0,
                cleaning: false,
                closed: false,
            })
        })();
        result.map_err(|error: String| {
            format!(
                "{error}; created development store retained at {}",
                root.display()
            )
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.identity.root
    }

    pub(super) fn next_directory(&mut self, label: &str) -> Result<PathBuf, String> {
        if self.closed || self.cleaning {
            return Err("the development store is closing or already closed".into());
        }
        if label.is_empty()
            || label.len() > 48
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(
                "development directory labels require 1..=48 ASCII letters, digits or '-'".into(),
            );
        }
        if self.next >= MAX_DIRECTORIES {
            return Err("the development store exhausted its 64 directory allowance".into());
        }
        self.identity.verify(false)?;
        self.next += 1;
        let path = self.root().join(format!("{:02}-{label}", self.next));
        create_private_directory(&path)?;
        let resolved = fs::canonicalize(&path)
            .map_err(|error| format!("resolve created directory {}: {error}", path.display()))?;
        if resolved != path || resolved.parent() != Some(self.root()) {
            return Err(format!(
                "created directory escaped its development store: {}",
                path.display()
            ));
        }
        Ok(path)
    }

    pub(super) fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        if Arc::strong_count(&self.identity) != 1 {
            return Err(format!(
                "development store still has executable or process owners; retained {}",
                self.root().display(),
            ));
        }
        self.identity.verify(self.cleaning)?;
        let identity = Arc::get_mut(&mut self.identity)
            .ok_or("development store acquired another live owner before cleanup")?;
        drop(identity.marker_lease.take());
        drop(identity.directory_lease.take());
        self.cleaning = true;
        identity.verify(true)?;
        fs::remove_dir_all(&identity.root).map_err(|error| {
            format!(
                "development store cleanup incomplete; retained {}: {error}",
                identity.root.display()
            )
        })?;
        self.closed = true;
        Ok(())
    }
}

impl StoreIdentity {
    fn verify(&self, allow_missing_marker: bool) -> Result<(), String> {
        physical_directory(&self.named_parent)?;
        let parent = fs::canonicalize(&self.named_parent)
            .map_err(|error| format!("revalidate named development parent: {error}"))?;
        if parent != self.parent {
            return Err("the originally named development parent changed".into());
        }
        let current_parent = host_fs::readable_directory(&parent)
            .map_err(|error| format!("reopen named development parent: {error}"))?;
        if !current_parent
            .same_object(&self.parent_directory)
            .map_err(|error| format!("compare development parent identity: {error}"))?
        {
            return Err(
                "the development parent no longer identifies its original directory".into(),
            );
        }
        physical_directory(&self.root)?;
        let root = fs::canonicalize(&self.root)
            .map_err(|error| format!("resolve owned development tree: {error}"))?;
        if !root.is_absolute() || root != self.root || root.parent() != Some(parent.as_path()) {
            return Err(
                "the owned development tree is outside its original absolute parent".into(),
            );
        }
        let current = host_fs::readable_directory(&root)
            .map_err(|error| format!("reopen owned development tree: {error}"))?;
        if !current
            .same_object(&self.directory)
            .map_err(|error| format!("compare development store identity: {error}"))?
        {
            return Err(
                "the development store path no longer identifies its original directory".into(),
            );
        }
        match fs::symlink_metadata(root.join(STORE_MARKER)) {
            Err(error) if allow_missing_marker && error.kind() == io::ErrorKind::NotFound => {
                return Ok(());
            }
            Err(error) => return Err(format!("inspect development store marker: {error}")),
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err("the development store marker is not a physical regular file".into());
            }
            Ok(_) => {}
        }
        let marker = current
            .open_child_file(&component(STORE_MARKER)?)
            .map_err(|error| format!("reopen development store marker: {error}"))?;
        if !marker
            .same_object(&self.marker)
            .map_err(|error| format!("compare development marker identity: {error}"))?
        {
            return Err("the development store marker was replaced".into());
        }
        marker
            .verify_digest(self.marker_digest, MAX_IDENTITY_BYTES as u64)
            .map_err(|error| format!("verify development store marker: {error}"))
    }
}

pub(super) struct Executable {
    captured: CapturedCargoExecutable,
    path: PathBuf,
    directory: DirectoryCapability,
    file: FileCapability,
    _file_lease: RestrictiveCapabilityHandleLease,
    _directory_lease: RestrictiveCapabilityHandleLease,
    store: Arc<StoreIdentity>,
}

impl Executable {
    pub(super) fn install(
        store: &mut Store,
        captured: CapturedCargoExecutable,
    ) -> Result<Arc<Self>, String> {
        if captured.bytes().is_empty()
            || ContentDigest::of_bytes(captured.bytes()) != captured.digest()
        {
            return Err(
                "captured executable bytes do not match their nonempty content digest".into(),
            );
        }
        let root = store.next_directory("executable")?;
        let result = (|| {
            let name = component(&format!("continuity{}", std::env::consts::EXE_SUFFIX))?;
            let path = root.join(
                name.to_str()
                    .ok_or("installed executable filename is not UTF-8")?,
            );
            let directory = host_fs::writable_directory(&root)
                .map_err(|error| format!("open executable installation: {error}"))?;
            let mut output = directory
                .create_child_executable_file(&name)
                .map_err(|error| format!("create installed executable: {error}"))?;
            output
                .writer()
                .map_err(|error| format!("open installed executable writer: {error}"))?
                .write_all(captured.bytes())
                .map_err(|error| format!("write installed executable: {error}"))?;
            output
                .sync()
                .map_err(|error| format!("sync installed executable: {error}"))?;
            drop(output);
            drop(directory);
            let directory = host_fs::readable_directory(&root)
                .map_err(|error| format!("retain executable installation: {error}"))?;
            let file = directory
                .open_child_file(&name)
                .map_err(|error| format!("retain installed executable: {error}"))?;
            if file.rights() != CapabilityRights::ReadOnly
                || directory.options().rights() != CapabilityRights::ReadOnly
            {
                return Err("an installed executable must not retain writable capabilities".into());
            }
            let file_lease = file
                .duplicate_restrictive_host_handle()
                .map_err(|error| format!("retain immutable executable lease: {error}"))?;
            let directory_lease = directory
                .duplicate_restrictive_host_handle()
                .map_err(|error| format!("retain immutable installation lease: {error}"))?;
            let executable = Self {
                captured,
                path,
                directory,
                file,
                _file_lease: file_lease,
                _directory_lease: directory_lease,
                store: Arc::clone(&store.identity),
            };
            executable.verify()?;
            executable.verify_bytes()?;
            Ok(Arc::new(executable))
        })();
        result.map_err(|error: String| {
            format!("{error}; installation retained at {}", root.display())
        })
    }

    pub(super) fn captured(&self) -> &CapturedCargoExecutable {
        &self.captured
    }

    pub(super) fn digest(&self) -> ContentDigest {
        self.captured.digest()
    }

    fn verify_bytes(&self) -> Result<(), String> {
        let mut reader = self
            .file
            .reader()
            .map_err(|error| format!("read installed executable bytes: {error}"))?;
        let mut buffer = [0; 64 * 1024];
        for expected in self.captured.bytes().chunks(buffer.len()) {
            let actual = &mut buffer[..expected.len()];
            reader
                .read_exact(actual)
                .map_err(|error| format!("read exact installed executable bytes: {error}"))?;
            if actual != expected {
                return Err(
                    "installed executable bytes differ from the captured executable".into(),
                );
            }
        }
        if reader
            .read(&mut buffer[..1])
            .map_err(|error| format!("check installed executable length: {error}"))?
            != 0
        {
            return Err("installed executable grew beyond its captured bytes".into());
        }
        Ok(())
    }

    fn verify(&self) -> Result<(), String> {
        self.store.verify(false)?;
        let root = self
            .path
            .parent()
            .ok_or("installed executable has no immediate directory")?;
        physical_directory(root)?;
        if fs::canonicalize(root).map_err(|error| format!("resolve installation: {error}"))? != root
        {
            return Err("installed executable directory moved outside its original path".into());
        }
        let current_directory = host_fs::readable_directory(root)
            .map_err(|error| format!("reopen installed executable directory: {error}"))?;
        if !current_directory
            .same_object(&self.directory)
            .map_err(|error| format!("compare installed executable directory: {error}"))?
        {
            return Err("installed executable directory was replaced".into());
        }
        let name = self
            .path
            .file_name()
            .ok_or("installed executable has no filename")?;
        let current = current_directory
            .open_child_file(
                &RelativeComponent::new(name)
                    .map_err(|error| format!("invalid installed executable filename: {error}"))?,
            )
            .map_err(|error| format!("reopen installed executable: {error}"))?;
        if !current
            .same_object(&self.file)
            .map_err(|error| format!("compare installed executable object: {error}"))?
        {
            return Err("installed executable path was replaced".into());
        }
        self.file
            .verify_digest(self.digest(), self.captured.bytes().len() as u64)
            .map_err(|error| format!("verify installed executable content: {error}"))
    }

    fn launch_authority(&self) -> Result<ExactExecutableLaunchAuthority, String> {
        self.verify()?;
        let file = self
            .file
            .duplicate_restrictive_executable_host_handle()
            .map_err(|error| format!("lease exact executable for launch: {error}"))?;
        let directory = self
            .directory
            .duplicate_restrictive_host_handle()
            .map_err(|error| format!("lease exact immediate launch directory: {error}"))?;
        ExactExecutableLaunchAuthority::from_verified_handles(file, directory)
            .map_err(|error| format!("admit exact executable launch: {error}"))
    }
}

pub(super) struct StartFailure {
    pub message: String,
    pub process: Option<Process>,
}

impl From<String> for StartFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            process: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Retirement {
    pub native: SupervisedProcessCleanupReceipt,
    pub game_closed: bool,
    pub terminated: bool,
}

pub(super) struct Process {
    native: SupervisedProcessOwner,
    executable: Arc<Executable>,
    placement: DevelopmentPlacement,
    control: PathBuf,
    directory: DirectoryCapability,
    #[cfg(windows)]
    _control_lease: nara::fs::DirectoryNamespaceLease,
    session: String,
    generation: String,
    next_request: u64,
    fault: Option<String>,
    closing: bool,
    close_request: Option<u64>,
    close_result: Option<Result<bool, String>>,
    terminated: bool,
    cleanup_error: Option<String>,
    retired: Option<Retirement>,
}

struct PreparedLaunch {
    control: PathBuf,
    directory: DirectoryCapability,
    #[cfg(windows)]
    control_lease: nara::fs::DirectoryNamespaceLease,
    session: String,
    generation: String,
    command: SupervisedProductProcessCommand,
    limits: ProcessLogLimits,
}

impl Process {
    pub(super) fn start(
        project: &Path,
        store: &mut Store,
        executable: Arc<Executable>,
        placement: DevelopmentPlacement,
        timings: &mut Option<DevelopmentTimingSnapshot>,
    ) -> Result<Self, StartFailure> {
        let prepared = timing::measure(
            timings,
            DevelopmentTimingPhase::ChildNamespacePreparation,
            |_| -> Result<PreparedLaunch, StartFailure> {
                #[cfg(not(windows))]
                if placement == DevelopmentPlacement::SourceDesktop {
                    return Err(
                        "source desktop development processes are supported only on Windows"
                            .to_owned()
                            .into(),
                    );
                }
                if !project.is_absolute() {
                    return Err("the game project path must be absolute".to_owned().into());
                }
                if !Arc::ptr_eq(&store.identity, &executable.store) {
                    return Err(
                        "executable and control directory must belong to the same store"
                            .to_owned()
                            .into(),
                    );
                }
                let project = fs::canonicalize(project)
                    .map_err(|error| format!("resolve game project: {error}"))?;
                physical_directory(&project)?;
                if store.root().starts_with(&project) || project.starts_with(store.root()) {
                    return Err("the development store must be separate from project files"
                        .to_owned()
                        .into());
                }
                let control = store.next_directory("process")?;
                let session = format!("{}-{:x}", store.identity.session_prefix, store.next);
                let digest = executable.digest();
                let generation = format!("build-{:x}-{:?}", store.next, digest);
                if session.is_empty()
                    || session.len() > MAX_IDENTITY_BYTES
                    || generation.is_empty()
                    || generation.len() > MAX_IDENTITY_BYTES
                {
                    return Err("generated process identities exceeded their protocol limit"
                        .to_owned()
                        .into());
                }
                let directory = host_fs::writable_directory(&control)
                    .map_err(|error| format!("open process control directory: {error}"))?;
                directory
                    .preflight_replace(ExpectedTarget::Missing)
                    .map_err(|error| format!("preflight process control publication: {error}"))?;
                #[cfg(windows)]
                let control_lease = directory
                    .acquire_namespace_lease()
                    .map_err(|error| format!("retain process control directory: {error}"))?;
                #[cfg(windows)]
                let environment = SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession;
                #[cfg(not(windows))]
                let environment = SupervisedProductEnvironmentPolicy::Sealed;
                let command = SupervisedProductProcessCommand::new(environment, &control)
                    .arguments([
                        OsStr::new("--project-root"),
                        project.as_os_str(),
                        OsStr::new("--control-dir"),
                        control.as_os_str(),
                        OsStr::new("--session"),
                        OsStr::new(&session),
                        OsStr::new("--generation"),
                        OsStr::new(&generation),
                        OsStr::new("--placement"),
                        OsStr::new(placement.argument()),
                    ]);
                let limits = ProcessLogLimits::try_from_counts(MAX_LOG_RECORDS, MAX_LOG_BYTES)
                    .map_err(|error| format!("configure bounded process logs: {error}"))?;
                Ok(PreparedLaunch {
                    control,
                    directory,
                    #[cfg(windows)]
                    control_lease,
                    session,
                    generation,
                    command,
                    limits,
                })
            },
        )?;
        let authority = timing::measure(
            timings,
            DevelopmentTimingPhase::ChildLaunchAuthority,
            |_| executable.launch_authority(),
        )?;
        let native = timing::measure(timings, DevelopmentTimingPhase::ChildNativeSpawn, |_| {
            SupervisedProcessOwner::spawn_exact(authority, &prepared.command, prepared.limits)
                .map_err(|error| format!("spawn exact game process: {error}"))
        })?;
        let mut process = Self {
            native,
            executable,
            placement,
            control: prepared.control,
            directory: prepared.directory,
            #[cfg(windows)]
            _control_lease: prepared.control_lease,
            session: prepared.session,
            generation: prepared.generation,
            next_request: 1,
            fault: None,
            closing: false,
            close_request: None,
            close_result: None,
            terminated: false,
            cleanup_error: None,
            retired: None,
        };
        let startup = timing::measure(timings, DevelopmentTimingPhase::ChildReadyWait, |_| {
            process
                .wait_response(READY_FILE, 0, Instant::now() + START_TIMEOUT)
                .and_then(|response| match response.result {
                    Ok(Reply::Observed { observation })
                        if observation.phase == "running"
                            && !observation.restoring
                            && observation.error.is_none()
                            && observation.engine_tick == 0
                            && observation.logical_tick == 0
                            && observation.logical_elapsed_nanos == 0
                            && observation.runtime_generation != 0 =>
                    {
                        Ok(())
                    }
                    other => Err(format!(
                        "bootstrap did not observe a fresh initialized paused game: {other:?}"
                    )),
                })
        });
        if let Err(message) = startup {
            process.fence(&message);
            return Err(StartFailure {
                message,
                process: Some(process),
            });
        }
        Ok(process)
    }

    pub(super) fn pid(&self) -> u32 {
        self.native.process_id()
    }

    pub(super) fn fenced(&self) -> bool {
        self.fault.is_some()
    }

    pub(super) fn observation(&mut self) -> Result<Observation, String> {
        self.observed(Command::Observe, Instant::now() + REQUEST_TIMEOUT)
    }

    pub(super) fn execution_status(&mut self) -> Result<ExecutionStatus, String> {
        self.execution(Command::ExecutionStatus, None)
    }

    pub(super) fn pause(&mut self) -> Result<ExecutionStatus, String> {
        self.execution(Command::Pause, Some(ExecutionState::Paused))
    }

    pub(super) fn resume(&mut self) -> Result<ExecutionStatus, String> {
        self.execution(Command::Resume, Some(ExecutionState::Running))
    }

    fn execution(
        &mut self,
        command: Command,
        expected_state: Option<ExecutionState>,
    ) -> Result<ExecutionStatus, String> {
        if self.placement != DevelopmentPlacement::SourceDesktop {
            return Err(
                "continuous execution controls require the explicit desktop placement".into(),
            );
        }
        match self.request(command, Instant::now() + REQUEST_TIMEOUT)? {
            Reply::Execution { status } => {
                if expected_state.is_some_and(|expected| status.state != expected) {
                    return Err(self.protocol_fault(format!(
                        "execution control returned {:?}, expected {expected_state:?}",
                        status.state
                    )));
                }
                Ok(status)
            }
            reply => Err(self.protocol_fault(format!(
                "execution control returned an unexpected reply: {reply:?}"
            ))),
        }
    }

    pub(super) fn capture(&mut self) -> Result<ResumeState, String> {
        match self.request(Command::Capture, Instant::now() + REQUEST_TIMEOUT)? {
            Reply::Captured { state } => {
                crate::resume::encode_resume(&state).map_err(|error| {
                    self.protocol_fault(format!("the game returned invalid resume state: {error}"))
                })?;
                Ok(state)
            }
            reply => {
                Err(self.protocol_fault(format!("capture returned an unexpected reply: {reply:?}")))
            }
        }
    }

    pub(super) fn step(&mut self, commands: Vec<SemanticCommand>) -> Result<Observation, String> {
        if commands.len() > MAX_STEP_COMMANDS || commands.iter().any(|command| {
            matches!(command, SemanticCommand::Move { direction, .. } if *direction >= 4)
        }) {
            return Err("a step requires at most eight commands and movement directions below four".into());
        }
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let mut observation = self.observed(Command::Step { commands }, deadline)?;
        for frame in 0..=MAX_SETTLE_FRAMES {
            if let Some(error) = &observation.error {
                return Err(format!(
                    "the game owner reported an operation error: {error}"
                ));
            }
            if observation.phase == "running" && !observation.restoring {
                return Ok(observation);
            }
            if Instant::now() >= deadline {
                return Err(
                    self.protocol_fault("game maintenance exceeded its settling deadline".into())
                );
            }
            if frame == MAX_SETTLE_FRAMES {
                break;
            }
            thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
            observation = self.observed(Command::Frame, deadline)?;
        }
        Err(self.protocol_fault("game maintenance exhausted its finite Frame allowance".into()))
    }

    pub(super) fn restore(&mut self, state: &ResumeState) -> Result<Observation, String> {
        self.observed(
            Command::Restore {
                state: state.clone(),
            },
            Instant::now() + REQUEST_TIMEOUT,
        )
    }

    pub(super) fn confirm_presentation(&mut self) -> Result<DesktopPresentation, String> {
        match self.request(
            Command::ConfirmPresentation,
            Instant::now() + REQUEST_TIMEOUT,
        )? {
            Reply::Presented { presentation } => Ok(presentation),
            reply => Err(self.protocol_fault(format!(
                "presentation confirmation returned an unexpected reply: {reply:?}"
            ))),
        }
    }

    pub(super) fn isolate_input(&mut self) -> Result<Observation, String> {
        self.observed(Command::IsolateInput, Instant::now() + REQUEST_TIMEOUT)
    }

    pub(super) fn activate_input(&mut self) -> Result<(InputActivation, Option<String>), String> {
        match self.request(Command::ActivateInput, Instant::now() + REQUEST_TIMEOUT)? {
            Reply::Activated { activation, error } => Ok((activation, error)),
            reply => Err(self.protocol_fault(format!(
                "input activation returned an unexpected reply: {reply:?}"
            ))),
        }
    }

    fn observed(&mut self, command: Command, deadline: Instant) -> Result<Observation, String> {
        match self.request(command, deadline)? {
            Reply::Observed { observation } => Ok(observation),
            reply => {
                Err(self.protocol_fault(format!("expected a game observation, got {reply:?}")))
            }
        }
    }

    fn request(&mut self, command: Command, deadline: Instant) -> Result<Reply, String> {
        if let Some(fault) = &self.fault {
            return Err(format!("the process protocol is fenced: {fault}"));
        }
        let closing = matches!(&command, Command::Close);
        if self.closing && !closing {
            return Err("the process has begun physical retirement".into());
        }
        if self.next_request > MAX_REQUESTS
            || (!closing && self.next_request > MAX_REQUESTS - CLEANUP_REQUESTS)
        {
            return Err(
                "the finite process request allowance reserves its last eight requests for Close"
                    .into(),
            );
        }
        let packet = Request {
            version: PROTOCOL_VERSION,
            session: self.session.clone(),
            generation: self.generation.clone(),
            request: self.next_request,
            command,
        };
        let bytes = encode_request(&packet)?;
        self.next_request = self
            .next_request
            .checked_add(1)
            .ok_or("the process request sequence is exhausted")?;
        let response = match self.exchange(&packet, &bytes, deadline) {
            Ok(response) => response,
            Err(error) => return Err(self.protocol_fault(error)),
        };
        response.result
    }

    fn exchange(
        &mut self,
        packet: &Request,
        bytes: &[u8],
        deadline: Instant,
    ) -> Result<Response, String> {
        self.verify_control()?;
        self.check_running()?;
        loop {
            match fs::symlink_metadata(self.control.join(REQUEST_FILE)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                Err(error) => return Err(format!("inspect game request inbox: {error}")),
                Ok(metadata) if !metadata.file_type().is_file() => {
                    return Err("the game request inbox is not a physical regular file".into());
                }
                Ok(_) => {}
            }
            self.check_running()?;
            if Instant::now() >= deadline {
                return Err(
                    "the previous game request was not consumed before its deadline".into(),
                );
            }
            thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
        if Instant::now() >= deadline {
            return Err("the game request deadline elapsed before publication".into());
        }
        let name = component(&format!("host-request-{}.tmp", packet.request))?;
        let mut temporary = self
            .directory
            .create_temp(&name)
            .map_err(publication_failure)?;
        temporary
            .write_all(bytes)
            .and_then(|()| temporary.flush())
            .map_err(|error| format!("write game request: {error}"))?;
        self.directory
            .replace_temp(
                temporary,
                &component(REQUEST_FILE)?,
                ExpectedTarget::Missing,
            )
            .map_err(publication_failure)?;
        let response = self.wait_response(
            &format!("response-{}.json", packet.request),
            packet.request,
            deadline,
        )?;
        if let Ok(reply) = &response.result {
            let expected = match &packet.command {
                Command::Observe
                | Command::Step { .. }
                | Command::Frame
                | Command::Restore { .. }
                | Command::IsolateInput => matches!(reply, Reply::Observed { .. }),
                Command::ExecutionStatus | Command::Pause | Command::Resume => {
                    matches!(reply, Reply::Execution { .. })
                }
                Command::Capture => matches!(reply, Reply::Captured { .. }),
                Command::ConfirmPresentation => matches!(reply, Reply::Presented { .. }),
                Command::ActivateInput => matches!(reply, Reply::Activated { .. }),
                Command::Close => matches!(reply, Reply::Closing | Reply::Closed { .. }),
            };
            if !expected {
                return Err(format!(
                    "unexpected game reply for {:?}: {reply:?}",
                    packet.command
                ));
            }
        }
        Ok(response)
    }

    fn wait_response(
        &mut self,
        name: &str,
        request: u64,
        deadline: Instant,
    ) -> Result<Response, String> {
        loop {
            self.verify_control()?;
            let exited = self.advance()?;
            let mut bytes = self.read_packet(name)?;
            if exited && bytes.is_none() {
                bytes = self.read_packet(name)?;
            }
            if let Some(bytes) = bytes {
                let response = decode_response(&bytes)?;
                if response.version != PROTOCOL_VERSION
                    || response.placement != self.placement
                    || response.session != self.session
                    || response.generation != self.generation
                    || response.request != request
                    || response.session.is_empty()
                    || response.session.len() > MAX_IDENTITY_BYTES
                    || response.generation.is_empty()
                    || response.generation.len() > MAX_IDENTITY_BYTES
                {
                    return Err(format!(
                        "stale protocol reply for original process request {request}"
                    ));
                }
                if Instant::now() >= deadline {
                    return Err(format!("game reply {name} arrived after its deadline"));
                }
                fs::remove_file(self.control.join(name))
                    .map_err(|error| format!("consume game reply {name}: {error}"))?;
                return Ok(response);
            }
            if exited {
                return Err(format!(
                    "native process exited before game reply {name}: {}",
                    self.diagnostics()
                ));
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "game reply {name} exceeded its deadline: {}",
                    self.diagnostics()
                ));
            }
            thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }

    fn verify_control(&self) -> Result<(), String> {
        self.executable.store.verify(false)?;
        physical_directory(&self.control)?;
        if fs::canonicalize(&self.control)
            .map_err(|error| format!("resolve process control directory: {error}"))?
            != self.control
        {
            return Err("process control directory moved outside its original path".into());
        }
        let current = host_fs::readable_directory(&self.control)
            .map_err(|error| format!("reopen process control directory: {error}"))?;
        if !current
            .same_object(&self.directory)
            .map_err(|error| format!("compare original process control directory: {error}"))?
        {
            return Err("process control directory was replaced".into());
        }
        Ok(())
    }

    fn read_packet(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        match fs::symlink_metadata(self.control.join(name)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("inspect game reply {name}: {error}")),
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(format!("game reply {name} is not a physical regular file"));
            }
            Ok(_) => {}
        }
        let file = self
            .directory
            .open_child_file(&component(name)?)
            .map_err(|error| format!("open game reply {name}: {error}"))?;
        file.read_to_end_bounded(MAX_MESSAGE_BYTES as u64)
            .map(Some)
            .map_err(|error| format!("read bounded game reply {name}: {error}"))
    }

    fn advance(&mut self) -> Result<bool, String> {
        let observation = self
            .native
            .advance(Instant::now())
            .map_err(|error| format!("advance original native process: {error}"))?;
        self.cache_retirement();
        Ok(observation.exit().is_some())
    }

    fn check_running(&mut self) -> Result<(), String> {
        if self.advance()? || self.native.state() != SupervisedProcessState::Running {
            return Err(format!(
                "the original native game is no longer running; {}",
                self.diagnostics(),
            ));
        }
        Ok(())
    }

    fn fence(&mut self, error: &str) {
        if self.fault.is_none() {
            self.fault = Some(error.to_owned());
        }
    }

    fn protocol_fault(&mut self, error: String) -> String {
        self.fence(&error);
        error
    }

    /// A pending game cleanup retains this process for the caller's next explicit Close retry.
    /// `terminated` records fallback termination, independently of the game's Close reply.
    pub(super) fn close(&mut self, deadline: Instant) -> Result<Retirement, String> {
        if let Err(error) = self.advance() {
            self.fence(&error);
            self.cleanup_error = Some(error);
        }
        self.cache_retirement();
        if let Some(retirement) = self.retired {
            return Ok(retirement);
        }
        self.closing = true;
        let now = Instant::now();
        if now >= deadline {
            return Err(
                "original native cleanup has no remaining caller drive time; custody retained"
                    .into(),
            );
        }
        let cooperative_deadline = deadline.min(now + COOPERATIVE_CLOSE_TIMEOUT);
        if !self.fenced() && self.close_result.is_none() {
            if self.next_request > MAX_REQUESTS {
                return Err(
                    "the game exhausted its eight reserved Close requests; cleanup remains pending and custody is retained"
                        .into(),
                );
            }
            self.close_request = Some(self.next_request);
            match self.request(Command::Close, cooperative_deadline) {
                Ok(Reply::Closing) => {
                    self.close_request = None;
                    return Err(format!(
                        "original game cleanup remains pending; custody retained for explicit Close retry: {}",
                        self.diagnostics(),
                    ));
                }
                Ok(Reply::Closed { failed }) => self.close_result = Some(Ok(failed)),
                Ok(reply) => {
                    self.close_result = Some(Err(
                        self.protocol_fault(format!("unexpected Close reply: {reply:?}"))
                    ));
                }
                Err(error) => self.close_result = Some(Err(error)),
            }
            self.close_request = None;
        }
        let mut fallback = self.fenced() || !matches!(self.close_result, Some(Ok(_)));
        let mut retry = self.cleanup_error.is_some();
        let mut retry_at = Instant::now();
        loop {
            let now = Instant::now();
            if let Err(error) = self.advance() {
                self.fence(&error);
                self.cleanup_error = Some(error);
                fallback = true;
                retry = true;
            }
            self.cache_retirement();
            if let Some(retirement) = self.retired {
                return Ok(retirement);
            }
            if !self.terminated && (fallback || now >= cooperative_deadline) {
                self.terminated = true;
                if let Err(error) = self.native.request_terminate(now) {
                    self.cleanup_error =
                        Some(format!("request native fallback termination: {error}"));
                    retry = true;
                }
            }
            if now >= deadline {
                return Err(format!(
                    "original native cleanup remains incomplete at its deadline; custody retained: {}",
                    self.diagnostics(),
                ));
            }
            if retry && now >= retry_at {
                match self.native.retry_cleanup(now) {
                    Ok(()) => retry = false,
                    Err(error) => {
                        self.cleanup_error = Some(format!("retry original native cleanup: {error}"))
                    }
                }
                retry_at = now + CLEANUP_RETRY_INTERVAL;
            }
            thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }

    fn cache_retirement(&mut self) {
        if self.close_request.is_some() {
            return;
        }
        if let Some(native) = self.native.cleanup_receipt()
            && self.native.state() == SupervisedProcessState::Complete
            && native.containment_empty()
            && native.readers_stopped()
            && native.output_drained()
        {
            self.retired = Some(Retirement {
                native,
                game_closed: matches!(self.close_result, Some(Ok(false))),
                terminated: self.terminated || native.force_killed(),
            });
        }
    }

    pub(super) fn diagnostics(&self) -> String {
        let mut text = format!(
            "bounded diagnostic tail (records<={MAX_LOG_RECORDS}, bytes<={MAX_LOG_BYTES}): {:?}; placement={:?}; output_loss={}; native_state={:?}; cleanup_receipt={:?}; protocol_fault={:?}; closing={}; close_request={:?}; close_result={:?}; fallback_termination_requested={}; cleanup_error={:?}",
            self.native.logs().stats(),
            self.placement,
            self.native.output_loss_observed(),
            self.native.state(),
            self.native.cleanup_receipt(),
            self.fault,
            self.closing,
            self.close_request,
            self.close_result,
            self.terminated,
            self.cleanup_error,
        );
        for entry in self.native.logs().iter() {
            text.push_str(&format!(
                "\n{:?} truncated_prefix_bytes={}: {}",
                entry.stream(),
                entry.truncated_prefix_bytes(),
                String::from_utf8_lossy(entry.bytes()),
            ));
        }
        text
    }
}

fn create_private_directory(path: &Path) -> Result<(), String> {
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder
        .create(path)
        .map_err(|error| format!("create exclusive directory {}: {error}", path.display()))
}

fn physical_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect directory {}: {error}", path.display()))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "expected a physical non-symlink directory: {}",
            path.display()
        ));
    }
    Ok(())
}

fn component(name: &str) -> Result<RelativeComponent, String> {
    RelativeComponent::new(name).map_err(|error| format!("invalid development filename: {error}"))
}

fn publication_failure(mut failure: TemporaryFileFailure) -> String {
    let complete = failure.retry_cleanup();
    format!("publish game request: {failure}; temporary cleanup complete={complete}")
}

fn encode_request(packet: &Request) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0; MAX_MESSAGE_BYTES];
    let length = {
        let mut output = io::Cursor::new(bytes.as_mut_slice());
        serde_json::to_writer(&mut output, packet)
            .map_err(|error| format!("encode bounded game request: {error}"))?;
        output.position() as usize
    };
    bytes.truncate(length);
    preflight(&bytes, MAX_IDENTITY_BYTES)?;
    Ok(bytes)
}

fn decode_response(bytes: &[u8]) -> Result<Response, String> {
    preflight(bytes, 16 * 1024)?;
    serde_json::from_slice(bytes).map_err(|error| format!("decode game reply: {error}"))
}

fn preflight(bytes: &[u8], maximum_string: usize) -> Result<(), String> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("game packet exceeds MAX_MESSAGE_BYTES".into());
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(20).ok_or("invalid packet depth limit")?,
        ItemLimit::new(8192).ok_or("invalid packet node limit")?,
        ItemLimit::new(256).ok_or("invalid packet container limit")?,
        ByteLimit::new(maximum_string).ok_or("invalid packet string limit")?,
        ByteLimit::new(64 * 1024).ok_or("invalid packet total string limit")?,
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits)
        .map_err(|error| format!("game packet shape was refused: {error}"))?;
    deserializer
        .end()
        .map_err(|error| format!("invalid complete game packet JSON: {error}"))
}
