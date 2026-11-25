use std::{
    ffi::OsString,
    io::{self, PipeReader, PipeWriter, Read},
    mem::MaybeUninit,
    os::{fd::AsRawFd, unix::process::CommandExt as _},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
};

use libc::{SIGKILL, SIGTERM, pid_t};

pub(super) struct Process {
    group: pid_t,
    child: Option<Child>,
}

impl Process {
    pub(super) fn leader_exited(&mut self) -> io::Result<bool> {
        if self.child.is_none() {
            return Err(io::Error::from_raw_os_error(libc::ECHILD));
        }
        let mut info = MaybeUninit::<libc::siginfo_t>::zeroed();
        loop {
            // SAFETY: The group ID is also the owned child's PID and `info` is writable. WNOWAIT
            // keeps the exited leader waitable so its numeric group ID cannot be reused yet.
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.group as libc::id_t,
                    info.as_mut_ptr(),
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result == 0 {
                // SAFETY: `info` started zeroed, and waitid only writes a signal information record.
                return Ok(unsafe { info.assume_init().si_pid() } != 0);
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return self.check_wait_error(Err(error));
        }
    }

    pub(super) fn reap_if_idle(&mut self) -> io::Result<Option<ExitStatus>> {
        if group_has_active_members(self.group)? {
            return Ok(None);
        }
        // Signal before releasing the leader in case a member forked during the scan.
        self.terminate(true)?;
        if group_has_active_members(self.group)? {
            return Ok(None);
        }
        let Some(child) = self.child.as_mut() else {
            return Err(io::Error::from_raw_os_error(libc::ECHILD));
        };
        let status = child.try_wait();
        if matches!(status, Ok(Some(_))) {
            self.child = None;
        }
        self.check_wait_error(status)
    }

    pub(super) fn terminate(&mut self, force: bool) -> io::Result<()> {
        if self.child.is_none() {
            return Err(io::Error::from_raw_os_error(libc::ECHILD));
        }
        let signal = if force { SIGKILL } else { SIGTERM };
        // SAFETY: `group` is positive and the original leader remains owned while signalling it.
        if unsafe { libc::killpg(self.group, signal) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        }
    }

    fn check_wait_error<T>(&mut self, result: io::Result<T>) -> io::Result<T> {
        if result
            .as_ref()
            .is_err_and(|error| error.raw_os_error() == Some(libc::ECHILD))
        {
            self.child = None;
        }
        result
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.child.is_none() {
            return;
        }
        let exited = self.leader_exited();
        let killed = self.terminate(true).is_ok();
        if let Some(mut child) = self.child.take()
            && (killed || matches!(exited, Ok(true)) || child.kill().is_ok())
        {
            let _ = child.wait();
        }
    }
}

pub(super) fn spawn(
    directory: &Path,
    executable: &Path,
    arguments: &[OsString],
    stdout: PipeWriter,
    stderr: PipeWriter,
) -> io::Result<Process> {
    let mut command = Command::new(executable);
    command
        .current_dir(directory)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    // SAFETY: setsid is async-signal-safe and creates the child's group before exec. Its session
    // leadership also keeps the direct child in that group for its lifetime.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let child = command.spawn()?;
    Ok(Process {
        group: child.id() as pid_t,
        child: Some(child),
    })
}

pub(super) fn set_nonblocking(reader: &PipeReader) -> io::Result<()> {
    let descriptor = reader.as_raw_fd();
    // SAFETY: The reader owns a valid descriptor, and F_GETFL takes no third argument.
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: F_SETFL takes the integer file status flags of the same live descriptor.
    if unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn read_pipe(reader: &mut PipeReader, buffer: &mut [u8]) -> io::Result<usize> {
    reader.read(buffer)
}

#[cfg(target_os = "linux")]
fn group_has_active_members(group: pid_t) -> io::Result<bool> {
    use std::fs;

    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<pid_t>().ok())
        else {
            continue;
        };
        if pid <= 0 || pid == group {
            continue;
        }
        // SAFETY: getpgid observes the group of a positive PID and writes no memory.
        let current_group = unsafe { libc::getpgid(pid) };
        if current_group == -1 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
        }
        if current_group != group {
            continue;
        }
        match fs::read(entry.path().join("stat")) {
            Ok(stat) => {
                let fields = stat
                    .rsplitn(2, |byte| *byte == b')')
                    .next()
                    .unwrap_or_default();
                if !matches!(fields.get(1), Some(b'Z' | b'X' | b'x')) {
                    return Ok(true);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn group_has_active_members(group: pid_t) -> io::Result<bool> {
    let mut members = vec![0 as pid_t; 32];
    loop {
        let bytes = i32::try_from(std::mem::size_of_val(members.as_slice())).map_err(|_| {
            io::Error::other("the Play group member list exceeds its native size limit")
        })?;
        // SAFETY: The buffer holds `bytes` writable bytes. errno is thread-local and cleared to
        // distinguish an empty group from a libproc failure, both represented by zero.
        let count = unsafe {
            *libc::__error() = 0;
            libc::proc_listpgrppids(group, members.as_mut_ptr().cast(), bytes)
        };
        if count <= 0 {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(0) {
                Ok(false)
            } else {
                Err(error)
            };
        }
        if count as usize >= members.len() {
            members.resize(members.len() * 2, 0);
            continue;
        }
        for &pid in &members[..count as usize] {
            if pid != group && macos_active_member(pid, group)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
}

#[cfg(target_os = "macos")]
fn macos_active_member(pid: pid_t, group: pid_t) -> io::Result<bool> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let bytes = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: PROC_PIDTBSDINFO writes at most the supplied buffer size for one proc_bsdinfo.
    let result = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            bytes,
        )
    };
    if result == bytes {
        // SAFETY: proc_pidinfo reported writing the entire structure.
        let info = unsafe { info.assume_init() };
        return Ok(info.pbi_pgid == group as u32 && info.pbi_status != libc::SZOMB);
    }
    let error = io::Error::last_os_error();
    if result == 0 && error.raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else if result == 0 {
        Err(error)
    } else {
        Err(io::Error::other(
            "libproc reported an incomplete process record",
        ))
    }
}
