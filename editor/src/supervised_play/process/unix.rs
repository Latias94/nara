use std::io;

use nara::fs::RestrictiveCapabilityHandleLease;

use super::{
    PlatformOutput, SupervisedProcessCommand, SupervisedProcessStream,
    SupervisedProductEnvironmentPolicy, SupervisedProductProcessCommand,
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use super::output::PlatformOutputQueue;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::{
    ffi::{CString, OsStr, OsString, c_char, c_int, c_long, c_uint},
    fs,
    io::{Read, Write},
    os::{
        fd::{AsRawFd as _, OwnedFd},
        unix::{
            ffi::OsStrExt as _,
            net::UnixStream,
            process::{CommandExt, ExitStatusExt},
        },
    },
    process::{Child, ChildStderr, ChildStdout, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use rustix::{
    event::{PollFd, PollFlags, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    process::{
        Pid, Signal, WaitId, WaitIdOptions, WaitIdStatus, getpgid, kill_process_group, waitid,
    },
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_PENDING_OUTPUT_RECORDS: usize = 512;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_PENDING_OUTPUT_BYTES: usize = 256 * 1024;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_READ_BYTES_PER_STREAM_PER_WAKE: usize = 64 * 1024;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const OUTPUT_READ_CHUNK_BYTES: usize = 8 * 1024;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const OUTPUT_POLL_RECORD_BUDGET: usize = 64;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SPAWN_ROLLBACK_WAIT: Duration = Duration::from_millis(500);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SPAWN_ROLLBACK_POLL: Duration = Duration::from_millis(10);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SYS_CLOSE_RANGE_X86_64: c_long = 436;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const CLOSE_RANGE_CLOEXEC: c_uint = 1 << 2;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SYS_EXECVEAT_X86_64: c_long = 322;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const AT_EMPTY_PATH: c_int = 0x1000;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_PRODUCT_ENVIRONMENT_BYTES: usize = 64 * 1024;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const EMPTY_PATH: [c_char; 1] = [0];

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe extern "C" {
    fn syscall(number: c_long, ...) -> c_long;
}

pub(super) struct ExactGenerationLaunchAuthority {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    executable: RestrictiveCapabilityHandleLease,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    _generation_directory: RestrictiveCapabilityHandleLease,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    _launch_manifest: RestrictiveCapabilityHandleLease,
}

impl ExactGenerationLaunchAuthority {
    pub(super) fn new(
        executable: RestrictiveCapabilityHandleLease,
        generation_directory: RestrictiveCapabilityHandleLease,
        launch_manifest: RestrictiveCapabilityHandleLease,
    ) -> io::Result<Self> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return Ok(Self {
                executable,
                _generation_directory: generation_directory,
                _launch_manifest: launch_manifest,
            });
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = (executable, generation_directory, launch_manifest);
            Err(unsupported_platform())
        }
    }
}

pub(super) struct SpawnedProcess {
    pub(super) process: PlatformProcess,
    pub(super) bootstrap: Option<BootstrapChannel>,
}

pub(super) struct BootstrapChannel {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    stream: UnixStream,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone)]
pub(super) struct BootstrapCancellation {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    cancelled: Arc<AtomicBool>,
}

impl BootstrapCancellation {
    pub(super) fn cancel(&self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            self.cancelled.store(true, Ordering::Release);
            return Ok(());
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }
}

impl BootstrapChannel {
    pub(super) fn cancellation(&self) -> BootstrapCancellation {
        BootstrapCancellation {
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            cancelled: Arc::clone(&self.cancelled),
        }
    }

    pub(super) fn write_some(&mut self, bytes: &[u8]) -> io::Result<usize> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "the supervised bootstrap write was cancelled",
                ));
            }
            return match self.stream.write(bytes) {
                Ok(written) => Ok(written),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(0),
                Err(error) => Err(error),
            };
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = bytes;
            Err(unsupported_platform())
        }
    }

    #[cfg(test)]
    pub(super) fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return self.stream.write_all(bytes);
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = bytes;
            Err(unsupported_platform())
        }
    }

    pub(super) fn try_read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return match self.stream.read(bytes) {
                Ok(0) => Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the supervised bootstrap endpoint closed",
                )),
                Ok(read) => Ok(read),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(0),
                Err(error) => Err(error),
            };
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = bytes;
            Err(unsupported_platform())
        }
    }
}

pub(super) struct PlatformProcess {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    child: Option<Child>,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    process_group: Pid,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    direct_child_exit: Option<std::process::ExitStatus>,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    direct_child_reaped: bool,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output: Arc<PlatformOutputQueue>,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_reader_stopped: Arc<AtomicBool>,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_poll_budget: usize,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_cancel: UnixStream,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_cancel_requested: bool,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_reader: Option<JoinHandle<()>>,
}

impl PlatformProcess {
    #[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
    pub(super) fn test_enqueue_loss(&self) {
        self.output
            .push_loss(
                SupervisedProcessStream::Stderr,
                b"[nara-editor: test output loss]\n".to_vec(),
            )
            .expect("the test loss marker is accepted");
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if let Some(status) = self.direct_child_exit {
                return Ok(Some(status));
            }
            if self.direct_child_reaped {
                return Err(process_owner_retired());
            }
            if self.child.is_none() {
                return Err(process_owner_retired());
            }
            // WNOWAIT observes the terminal status without releasing the leader PID. Keeping that
            // zombie waitable also keeps its numeric PGID unavailable for unrelated reuse while
            // descendant containment is still being signalled and inspected.
            let status = loop {
                match waitid(
                    WaitId::Pid(self.process_group),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
                ) {
                    Ok(status) => break status.map(waitid_exit_status).transpose()?,
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(rustix::io::Errno::CHILD) => {
                        // Another wait authority in this process violated the single-owner
                        // contract. Retire the numeric group immediately so no later cleanup path
                        // can signal a potentially reused PGID.
                        self.direct_child_reaped = true;
                        self.child.take();
                        return Err(io::Error::other(
                            "the supervised direct child was reaped outside its process owner",
                        ));
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            if let Some(status) = status {
                self.direct_child_exit = Some(status);
            }
            return Ok(status);
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn request_terminate(&mut self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.direct_child_reaped {
                return Ok(());
            }
            return signal_process_group(self.process_group, Signal::TERM);
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn force_kill(&mut self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.direct_child_reaped {
                return Ok(());
            }
            return signal_process_group(self.process_group, Signal::KILL);
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn try_output(&mut self) -> io::Result<Option<PlatformOutput>> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.output_poll_budget == 0 {
                self.output_poll_budget = OUTPUT_POLL_RECORD_BUDGET;
                return Ok(None);
            }
            return match self.output.pop()? {
                Some(output) => {
                    self.output_poll_budget -= 1;
                    Ok(Some(output))
                }
                None => {
                    self.output_poll_budget = OUTPUT_POLL_RECORD_BUDGET;
                    Ok(None)
                }
            };
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn cancel_output(&mut self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.output_cancel_requested {
                return Ok(());
            }
            match self.output_cancel.write(&[1]) {
                Ok(1) => {
                    self.output_cancel_requested = true;
                    Ok(())
                }
                Ok(_) => Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "the supervised process output cancellation endpoint closed",
                )),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::BrokenPipe
                    ) =>
                {
                    self.output_cancel_requested = true;
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn readers_stopped(&mut self) -> bool {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let stopped = self.output_reader_stopped.load(Ordering::Acquire);
            let pending_output = self.output.has_pending();
            if !stopped || pending_output {
                return false;
            }
            let Some(reader) = self.output_reader.take() else {
                return true;
            };
            if reader.join().is_ok() {
                return true;
            }
            self.output.record_failure(io::Error::other(
                "the supervised process output reader panicked",
            ));
            false
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            false
        }
    }

    pub(super) fn containment_empty(&mut self) -> io::Result<bool> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.direct_child_reaped {
                return Ok(true);
            }
            if self.direct_child_exit.is_none() {
                return Ok(false);
            }
            // The process-group leader deliberately remains waitable until no other member can
            // execute. Only then is the direct child reaped and the PGID allowed to retire.
            if process_group_has_other_members(self.process_group)? {
                return Ok(false);
            }
            return self.reap_direct_child();
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn fail_safe_terminate(&mut self) {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let _ = self.cancel_output();
            if !self.direct_child_reaped {
                let _ = signal_process_group(self.process_group, Signal::KILL);
            }
            let Some(mut child) = self.child.take() else {
                return;
            };
            if child.try_wait().ok().flatten().is_some() {
                self.direct_child_reaped = true;
                return;
            }
            if let Err(error) = transfer_child_to_reaper(child, "nara-supervised-process-reaper") {
                self.output.record_failure(error);
            }
        }
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn reap_direct_child(&mut self) -> io::Result<bool> {
        let reaped = match self
            .child
            .as_mut()
            .ok_or_else(process_owner_retired)?
            .try_wait()
        {
            Ok(Some(reaped)) => reaped,
            Ok(None) => return Ok(false),
            Err(error)
                if rustix::io::Errno::from_io_error(&error) == Some(rustix::io::Errno::CHILD) =>
            {
                self.direct_child_reaped = true;
                self.child.take();
                return Err(io::Error::other(
                    "the supervised direct child was reaped outside its process owner",
                ));
            }
            Err(error) => return Err(error),
        };
        let status_mismatch = self
            .direct_child_exit
            .is_some_and(|observed| observed.into_raw() != reaped.into_raw());
        self.direct_child_exit = Some(reaped);
        self.direct_child_reaped = true;
        self.child.take();
        if status_mismatch {
            return Err(io::Error::other(
                "the reaped direct-child status differs from the non-reaping observation",
            ));
        }
        Ok(true)
    }
}

pub(super) fn spawn(
    command: &SupervisedProcessCommand,
    wake: crate::supervised_play::wake::SupervisedPlayWake,
) -> io::Result<SpawnedProcess> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return spawn_supported(command, wake);
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = (command, wake);
        Err(unsupported_platform())
    }
}

pub(super) fn spawn_exact_generation(
    authority: ExactGenerationLaunchAuthority,
    command: &SupervisedProductProcessCommand,
    wake: crate::supervised_play::wake::SupervisedPlayWake,
) -> io::Result<SpawnedProcess> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return spawn_exact_generation_supported(authority, command, wake);
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = (authority, command, wake);
        Err(unsupported_platform())
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn spawn_supported(
    command: &SupervisedProcessCommand,
    wake: crate::supervised_play::wake::SupervisedPlayWake,
) -> io::Result<SpawnedProcess> {
    let mut native = Command::new(command.executable());
    native
        .args(command.arguments_ref())
        .current_dir(command.working_directory())
        .envs(
            command
                .environment_ref()
                .iter()
                .map(|(name, value)| (name, value)),
        );
    spawn_native(
        native,
        command.bootstrap_requested(),
        wake,
        close_non_stdio_on_exec,
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn spawn_exact_generation_supported(
    authority: ExactGenerationLaunchAuthority,
    command: &SupervisedProductProcessCommand,
    wake: crate::supervised_play::wake::SupervisedPlayWake,
) -> io::Result<SpawnedProcess> {
    let executable_fd = authority.executable.as_raw_fd();
    let payload = ExactExecPayload::new(command)?;
    let mut native = Command::new("/proc/self/exe");
    native.current_dir(command.working_directory()).env_clear();

    let spawned = spawn_native(native, true, wake, move || {
        close_non_stdio_on_exec()?;
        // SAFETY: the executable descriptor and every C string/pointer array remain live through
        // this call. `AT_EMPTY_PATH` instructs Linux to execute the already-opened object rather
        // than resolving a mutable name. Success replaces the child image and never returns.
        let _ = unsafe {
            syscall(
                SYS_EXECVEAT_X86_64,
                executable_fd,
                EMPTY_PATH.as_ptr(),
                payload.argv_ptr(),
                payload.environment_ptr(),
                AT_EMPTY_PATH,
            )
        };
        Err(io::Error::last_os_error())
    });
    drop(authority);
    spawned
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn spawn_native(
    mut native: Command,
    bootstrap_requested: bool,
    wake: crate::supervised_play::wake::SupervisedPlayWake,
    child_setup: impl FnMut() -> io::Result<()> + Send + Sync + 'static,
) -> io::Result<SpawnedProcess> {
    let (bootstrap, child_bootstrap) = if bootstrap_requested {
        let (parent, child) = UnixStream::pair()?;
        parent.set_nonblocking(true)?;
        let child: OwnedFd = child.into();
        (
            Some(BootstrapChannel {
                stream: parent,
                cancelled: Arc::new(AtomicBool::new(false)),
            }),
            Stdio::from(child),
        )
    } else {
        (None, Stdio::null())
    };
    let (output_cancel, output_cancel_reader) = UnixStream::pair()?;
    output_cancel.set_nonblocking(true)?;
    output_cancel_reader.set_nonblocking(true)?;

    native
        .stdin(child_bootstrap)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    native.process_group(0);
    // SAFETY: `child_setup` is restricted to the platform module and performs only documented
    // async-signal-safe descriptor/process syscalls before either returning an error through
    // Command's private error pipe or replacing the child image.
    unsafe {
        native.pre_exec(child_setup);
    }

    let mut child = native.spawn()?;
    let process_group = Pid::from_child(&child);
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => return spawn_setup_failed(child, process_group, "stdout pipe was not created"),
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => return spawn_setup_failed(child, process_group, "stderr pipe was not created"),
    };

    if let Err(error) = make_nonblocking(&stdout).and_then(|()| make_nonblocking(&stderr)) {
        let cleanup = terminate_spawn_failure(child, process_group);
        return Err(merge_spawn_rollback_error(error, cleanup.err()));
    }
    let output = Arc::new(PlatformOutputQueue::with_wake(
        MAX_PENDING_OUTPUT_RECORDS,
        MAX_PENDING_OUTPUT_BYTES,
        wake,
    ));
    let output_reader_stopped = Arc::new(AtomicBool::new(false));
    let reader_output = Arc::clone(&output);
    let reader_stopped = Arc::clone(&output_reader_stopped);
    let output_reader = match thread::Builder::new()
        .name("nara-supervised-output".to_owned())
        .spawn(move || {
            run_output_reader(
                stdout,
                stderr,
                output_cancel_reader,
                reader_output,
                reader_stopped,
            );
        }) {
        Ok(reader) => reader,
        Err(error) => {
            let cleanup = terminate_spawn_failure(child, process_group);
            return Err(merge_spawn_rollback_error(error, cleanup.err()));
        }
    };

    Ok(SpawnedProcess {
        process: PlatformProcess {
            child: Some(child),
            process_group,
            direct_child_exit: None,
            direct_child_reaped: false,
            output,
            output_reader_stopped,
            output_poll_budget: OUTPUT_POLL_RECORD_BUDGET,
            output_cancel,
            output_cancel_requested: false,
            output_reader: Some(output_reader),
        },
        bootstrap,
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct ExactExecPayload {
    _arguments: Vec<CString>,
    argument_pointers: Box<[usize]>,
    _environment: Vec<CString>,
    environment_pointers: Box<[usize]>,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl ExactExecPayload {
    fn new(command: &SupervisedProductProcessCommand) -> io::Result<Self> {
        let mut arguments = Vec::with_capacity(command.arguments_ref().len().saturating_add(1));
        arguments.push(CString::new("nara-supervised-product").expect("fixed argv[0] has no NUL"));
        for argument in command.arguments_ref() {
            arguments.push(c_string(argument.as_os_str(), "product argument")?);
        }
        let environment = product_environment(command.environment_policy())?;
        let argument_pointers = pointer_array(&arguments);
        let environment_pointers = pointer_array(&environment);
        Ok(Self {
            _arguments: arguments,
            argument_pointers,
            _environment: environment,
            environment_pointers,
        })
    }

    fn argv_ptr(&self) -> *const *const c_char {
        self.argument_pointers.as_ptr().cast()
    }

    fn environment_ptr(&self) -> *const *const c_char {
        self.environment_pointers.as_ptr().cast()
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn pointer_array(strings: &[CString]) -> Box<[usize]> {
    strings
        .iter()
        .map(|value| value.as_ptr() as usize)
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn product_environment(policy: SupervisedProductEnvironmentPolicy) -> io::Result<Vec<CString>> {
    project_product_environment(policy, |name| std::env::var_os(name))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn project_product_environment(
    policy: SupervisedProductEnvironmentPolicy,
    mut value_for: impl FnMut(&str) -> Option<OsString>,
) -> io::Result<Vec<CString>> {
    let mut environment = Vec::with_capacity(policy.allowlisted_names().len());
    let mut encoded_bytes = 0_usize;
    for name in policy.allowlisted_names() {
        let Some(value) = value_for(name) else {
            continue;
        };
        let name_bytes = OsStr::new(name).as_bytes();
        let value_bytes = value.as_os_str().as_bytes();
        encoded_bytes = encoded_bytes
            .checked_add(name_bytes.len())
            .and_then(|total| total.checked_add(1))
            .and_then(|total| total.checked_add(value_bytes.len()))
            .and_then(|total| total.checked_add(1))
            .ok_or_else(product_environment_too_large)?;
        if encoded_bytes > MAX_PRODUCT_ENVIRONMENT_BYTES {
            return Err(product_environment_too_large());
        }
        let mut entry = Vec::with_capacity(name_bytes.len() + 1 + value_bytes.len());
        entry.extend_from_slice(name_bytes);
        entry.push(b'=');
        entry.extend_from_slice(value_bytes);
        environment.push(CString::new(entry).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "an allowlisted product environment value contains NUL",
            )
        })?);
    }
    Ok(environment)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn c_string(value: &OsStr, label: &'static str) -> io::Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the supervised {label} contains NUL"),
        )
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn product_environment_too_large() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "the allowlisted product environment exceeds its bounded limit",
    )
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod exact_launch_tests {
    use std::ffi::OsString;

    use super::{SupervisedProductEnvironmentPolicy, project_product_environment};

    #[test]
    fn desktop_product_environment_projects_only_reviewed_session_values() {
        let mut requested = Vec::new();
        let projected = project_product_environment(
            SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession,
            |name| {
                requested.push(name.to_owned());
                match name {
                    "DISPLAY" => Some(OsString::from(":19")),
                    "LANG" => Some(OsString::from("C.UTF-8")),
                    _ => None,
                }
            },
        )
        .expect("the reviewed environment projection is encodable");
        let entries = projected
            .iter()
            .map(|entry| entry.to_str().expect("the fixture environment is UTF-8"))
            .collect::<Vec<_>>();

        assert_eq!(entries, ["DISPLAY=:19", "LANG=C.UTF-8"]);
        assert!(!requested.iter().any(|name| matches!(
            name.as_str(),
            "PATH" | "HTTP_PROXY" | "LD_PRELOAD" | "LD_LIBRARY_PATH"
        )));

        let mut sealed_lookups = 0;
        let sealed =
            project_product_environment(SupervisedProductEnvironmentPolicy::Sealed, |_| {
                sealed_lookups += 1;
                Some(OsString::from("must-not-be-observed"))
            })
            .expect("the sealed environment is encodable");
        assert!(sealed.is_empty());
        assert_eq!(sealed_lookups, 0);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn spawn_setup_failed(
    child: Child,
    process_group: Pid,
    message: &'static str,
) -> io::Result<SpawnedProcess> {
    let cleanup = terminate_spawn_failure(child, process_group);
    Err(merge_spawn_rollback_error(
        io::Error::other(message),
        cleanup.err(),
    ))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn terminate_spawn_failure(mut child: Child, process_group: Pid) -> io::Result<()> {
    let signal_error = signal_process_group(process_group, Signal::KILL).err();
    if signal_error.is_some() {
        let _ = child.kill();
    }
    let deadline = Instant::now() + SPAWN_ROLLBACK_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return signal_error.map_or(Ok(()), Err),
            Ok(None) if Instant::now() < deadline => thread::sleep(SPAWN_ROLLBACK_POLL),
            Ok(None) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                let ownership =
                    transfer_child_to_reaper(child, "nara-supervised-spawn-rollback-reaper");
                return Err(merge_cleanup_errors(
                    signal_error,
                    Some(error),
                    ownership.err(),
                ));
            }
        }
    }
    let ownership = transfer_child_to_reaper(child, "nara-supervised-spawn-rollback-reaper");
    Err(merge_cleanup_errors(
        signal_error,
        Some(io::Error::new(
            io::ErrorKind::TimedOut,
            "the supervised spawn rollback exceeded its synchronous reap deadline; child ownership was transferred to the rollback reaper",
        )),
        ownership.err(),
    ))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn waitid_exit_status(status: WaitIdStatus) -> io::Result<std::process::ExitStatus> {
    if let Some(code) = status.exit_status() {
        let raw = code.checked_shl(8).ok_or_else(|| {
            io::Error::other("the observed direct-child exit status exceeds wait status encoding")
        })?;
        return Ok(std::process::ExitStatus::from_raw(raw));
    }
    if let Some(signal) = status.terminating_signal() {
        let core_dumped = if status.dumped() { 0x80 } else { 0 };
        return Ok(std::process::ExitStatus::from_raw(signal | core_dumped));
    }
    Err(io::Error::other(
        "waitid returned a non-terminal direct-child status despite WEXITED",
    ))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn process_group_has_other_members(process_group: Pid) -> io::Result<bool> {
    let leader = process_group.as_raw_pid();
    // Linux process groups are not ownership handles. Enumerating live PIDs while the zombie
    // leader reserves the group number is the bounded bridge needed before final reap; any
    // inspection error is treated conservatively as a cleanup failure.
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(raw_pid) = name.parse::<i32>() else {
            continue;
        };
        if raw_pid == leader {
            continue;
        }
        let Some(pid) = Pid::from_raw(raw_pid) else {
            continue;
        };
        match getpgid(Some(pid)) {
            Ok(group) if group == process_group => return Ok(true),
            Ok(_) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn transfer_child_to_reaper(child: Child, thread_name: &'static str) -> io::Result<()> {
    let owner = Arc::new(std::sync::Mutex::new(Some(child)));
    let reaper_owner = Arc::clone(&owner);
    match thread::Builder::new()
        .name(thread_name.to_owned())
        .spawn(move || {
            let child = reaper_owner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
            if let Some(mut child) = child {
                let _ = wait_owned_child(&mut child);
            }
        }) {
        Ok(_reaper) => Ok(()),
        Err(spawn_error) => {
            let child = owner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
            let Some(mut child) = child else {
                return Err(io::Error::other(format!(
                    "the supervised child reaper could not start and child ownership was unavailable: {spawn_error}"
                )));
            };
            wait_owned_child(&mut child).map_err(|wait_error| {
                io::Error::other(format!(
                    "the supervised child reaper could not start ({spawn_error}) and synchronous reaping failed: {wait_error}"
                ))
            })
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn wait_owned_child(child: &mut Child) -> io::Result<()> {
    loop {
        match child.wait() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn merge_spawn_rollback_error(primary: io::Error, cleanup: Option<io::Error>) -> io::Error {
    let Some(cleanup) = cleanup else {
        return primary;
    };
    io::Error::new(
        primary.kind(),
        format!(
            "{primary}; supervised process spawn rollback required additional recovery: {cleanup}"
        ),
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn merge_cleanup_errors(
    first: Option<io::Error>,
    second: Option<io::Error>,
    third: Option<io::Error>,
) -> io::Error {
    let messages = [first, second, third]
        .into_iter()
        .flatten()
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    io::Error::other(messages.join("; "))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn make_nonblocking(fd: &impl std::os::fd::AsFd) -> io::Result<()> {
    let flags = fcntl_getfl(fd).map_err(io::Error::from)?;
    fcntl_setfl(fd, flags | OFlags::NONBLOCK).map_err(io::Error::from)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn signal_process_group(process_group: Pid, signal: Signal) -> io::Result<()> {
    match kill_process_group(process_group, signal) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn close_non_stdio_on_exec() -> io::Result<()> {
    // SAFETY: `syscall` receives the Linux x86-64 close_range number and integer-only arguments.
    // The hook runs after fork and before exec, where this syscall is async-signal-safe.
    let result = unsafe { syscall(SYS_CLOSE_RANGE_X86_64, 3_u32, u32::MAX, CLOSE_RANGE_CLOEXEC) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_output_reader(
    mut stdout: ChildStdout,
    mut stderr: ChildStderr,
    mut cancel: UnixStream,
    output: Arc<PlatformOutputQueue>,
    stopped: Arc<AtomicBool>,
) {
    let _stopped = OutputReaderStoppedGuard(stopped);
    let result = read_output_until_cancelled(&mut stdout, &mut stderr, &mut cancel, &output);
    if let Err(error) = result {
        output.record_failure(error);
    }
    output.notify();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct OutputReaderStoppedGuard(Arc<AtomicBool>);

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl Drop for OutputReaderStoppedGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn read_output_until_cancelled(
    stdout: &mut ChildStdout,
    stderr: &mut ChildStderr,
    cancel: &mut UnixStream,
    output: &PlatformOutputQueue,
) -> io::Result<()> {
    let mut stdout_open = true;
    let mut stderr_open = true;

    loop {
        if !stdout_open && !stderr_open {
            return Ok(());
        }

        let mut poll_fds = Vec::with_capacity(3);
        let cancel_index = poll_fds.len();
        poll_fds.push(PollFd::new(
            &*cancel,
            PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
        ));
        let stdout_index = stdout_open.then(|| {
            let index = poll_fds.len();
            poll_fds.push(PollFd::new(
                &*stdout,
                PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
            ));
            index
        });
        let stderr_index = stderr_open.then(|| {
            let index = poll_fds.len();
            poll_fds.push(PollFd::new(
                &*stderr,
                PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
            ));
            index
        });

        match poll(&mut poll_fds, None) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
        let cancel_events = poll_fds[cancel_index].revents();
        let stdout_events = stdout_index.map(|index| poll_fds[index].revents());
        let stderr_events = stderr_index.map(|index| poll_fds[index].revents());
        drop(poll_fds);

        if cancel_events.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR) {
            let mut discard = [0_u8; 32];
            let _ = cancel.read(&mut discard);
            if stdout_open && !drain_stream(stdout, SupervisedProcessStream::Stdout, output)? {
                record_cancelled_stream(output, SupervisedProcessStream::Stdout)?;
            }
            if stderr_open && !drain_stream(stderr, SupervisedProcessStream::Stderr, output)? {
                record_cancelled_stream(output, SupervisedProcessStream::Stderr)?;
            }
            return Ok(());
        }
        if stdout_events.is_some_and(|events| events.contains(PollFlags::NVAL))
            || stderr_events.is_some_and(|events| events.contains(PollFlags::NVAL))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a supervised process output descriptor became invalid",
            ));
        }
        if stdout_events.is_some_and(output_ready) {
            stdout_open = !drain_stream(stdout, SupervisedProcessStream::Stdout, output)?;
        }
        if stderr_events.is_some_and(output_ready) {
            stderr_open = !drain_stream(stderr, SupervisedProcessStream::Stderr, output)?;
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn output_ready(events: PollFlags) -> bool {
    events.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn drain_stream(
    reader: &mut impl Read,
    stream: SupervisedProcessStream,
    output: &PlatformOutputQueue,
) -> io::Result<bool> {
    let mut bytes_read = 0;
    let mut buffer = [0_u8; OUTPUT_READ_CHUNK_BYTES];
    while bytes_read < MAX_READ_BYTES_PER_STREAM_PER_WAKE {
        let remaining = MAX_READ_BYTES_PER_STREAM_PER_WAKE - bytes_read;
        let read_limit = remaining.min(buffer.len());
        match reader.read(&mut buffer[..read_limit]) {
            Ok(0) => return Ok(true),
            Ok(read) => {
                bytes_read += read;
                output.push(stream, buffer[..read].to_vec())?;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn record_cancelled_stream(
    output: &PlatformOutputQueue,
    stream: SupervisedProcessStream,
) -> io::Result<()> {
    let name = match stream {
        SupervisedProcessStream::Stdout => "stdout",
        SupervisedProcessStream::Stderr => "stderr",
    };
    output.push_loss(
        stream,
        format!(
            "[nara-editor: {name} capture was cancelled before EOF; remaining bytes were not retained]\n"
        )
        .into_bytes(),
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn process_owner_retired() -> io::Error {
    io::Error::new(
        io::ErrorKind::BrokenPipe,
        "the supervised process direct-child owner was already retired",
    )
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn unsupported_platform() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "supervised Unix processes are supported only on Linux x86-64",
    )
}
