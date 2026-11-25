//! Incremental Cargo builds and supervised external Play for trusted projects.

mod build;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "play/unix.rs"]
mod native;
#[cfg(windows)]
#[path = "play/windows.rs"]
mod native;
mod output;

use std::{
    ffi::OsString,
    io,
    path::{self, Path, PathBuf},
    process::ExitStatus,
    thread,
    time::{Duration, Instant},
};

pub use build::{
    BuildDiagnostic, BuildError, BuildEvent, BuildRequest, DiagnosticSpan, build,
    build_interruptible,
};
use output::LinePipe;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const TERMINATE_GRACE: Duration = Duration::from_millis(750);
const KILL_TIMEOUT: Duration = Duration::from_secs(3);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// One output line, decoded as UTF-8 with replacement for invalid bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLine {
    pub stream: OutputStream,
    pub text: String,
}

/// Owns one game process and its platform containment. Poll regularly to consume output and
/// finish natural exits. On Unix, descendants that leave the Play process group leave its scope.
pub struct PlayProcess {
    directory: PathBuf,
    executable: PathBuf,
    arguments: Vec<OsString>,
    run: Run,
}

impl PlayProcess {
    /// Launches a binary directly with closed stdin and the inherited environment. A relative
    /// executable path is resolved in `project_dir`.
    pub fn launch(
        project_dir: &Path,
        executable: &Path,
        arguments: &[OsString],
    ) -> io::Result<Self> {
        let directory = path::absolute(project_dir)?;
        let executable = directory.join(executable);
        if !executable.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the Play executable must resolve to an absolute path",
            ));
        }
        let run = Run::spawn(&directory, &executable, arguments)?;
        Ok(Self {
            directory,
            executable,
            arguments: arguments.to_vec(),
            run,
        })
    }

    /// Delivers available lines without waiting. A returned exit status means containment has
    /// finished, the leader has been reaped, and output delivery has completed.
    pub fn poll(&mut self, mut emit: impl FnMut(OutputLine)) -> io::Result<Option<ExitStatus>> {
        self.run.poll(&mut emit)
    }

    /// Waits for the game to exit, delivering output as it arrives.
    pub fn wait(&mut self, mut emit: impl FnMut(OutputLine)) -> io::Result<ExitStatus> {
        self.run.wait(|| false, &mut emit)
    }

    /// Stops and reaps the game and waits for active members of its containment to exit.
    pub fn stop(&mut self, mut emit: impl FnMut(OutputLine)) -> io::Result<ExitStatus> {
        self.run.wait(|| true, &mut emit)
    }

    /// Stops the previous run before launching the same command. For a rebuild, stop the game
    /// before calling [`build()`] so Cargo can replace the binary on Windows.
    pub fn restart(&mut self, emit: impl FnMut(OutputLine)) -> io::Result<()> {
        self.stop(emit)?;
        self.run = Run::spawn(&self.directory, &self.executable, &self.arguments)?;
        Ok(())
    }
}

pub(crate) fn capture_stdout(
    directory: &Path,
    executable: &Path,
    arguments: &[OsString],
    limit: usize,
    stop: impl FnMut() -> bool,
    mut emit_stderr: impl FnMut(OutputLine),
) -> io::Result<(ExitStatus, Vec<u8>)> {
    let mut run = Run::spawn(directory, executable, arguments)?;
    let mut bytes = Vec::new();
    let status = run.wait_with(stop, |run| {
        let stdout = run.stdout.drain_raw(run.exit, |chunk| {
            if chunk.len() > limit - bytes.len() {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            bytes.extend_from_slice(chunk);
            Ok(())
        });
        let stderr = run.stderr.drain(run.exit, &mut emit_stderr);
        stdout.and(stderr)
    })?;
    Ok((status, bytes))
}

struct Run {
    child: Option<native::Process>,
    stdout: LinePipe,
    stderr: LinePipe,
    shutdown: Option<Shutdown>,
    exit: Option<(ExitStatus, Instant)>,
}

#[derive(Clone, Copy)]
enum Shutdown {
    Terminating(Instant),
    Killing(Instant),
}

impl Run {
    fn spawn(directory: &Path, executable: &Path, arguments: &[OsString]) -> io::Result<Self> {
        let (stdout, stdout_writer) = LinePipe::new(OutputStream::Stdout)?;
        let (stderr, stderr_writer) = LinePipe::new(OutputStream::Stderr)?;
        let child = native::spawn(
            directory,
            executable,
            arguments,
            stdout_writer,
            stderr_writer,
        )?;
        Ok(Self {
            child: Some(child),
            stdout,
            stderr,
            shutdown: None,
            exit: None,
        })
    }

    fn poll(&mut self, emit: &mut impl FnMut(OutputLine)) -> io::Result<Option<ExitStatus>> {
        let lifecycle = self.advance(false);
        let output = self.drain(emit);
        lifecycle.and(output)?;
        Ok(self.finished())
    }

    fn wait(
        &mut self,
        stop: impl FnMut() -> bool,
        emit: &mut impl FnMut(OutputLine),
    ) -> io::Result<ExitStatus> {
        self.wait_with(stop, |run| run.drain(emit))
    }

    fn wait_with(
        &mut self,
        mut stop: impl FnMut() -> bool,
        mut drain: impl FnMut(&mut Self) -> io::Result<()>,
    ) -> io::Result<ExitStatus> {
        let mut output_error = None;
        loop {
            let lifecycle = self.advance(stop() || output_error.is_some());
            if let Err(error) = drain(self) {
                output_error.get_or_insert(error);
            }
            lifecycle?;
            if let Some(status) = self.finished() {
                return output_error.map_or(Ok(status), Err);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn advance(&mut self, stop: bool) -> io::Result<()> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        let leader_exited = child.leader_exited()?;
        let now = Instant::now();
        if leader_exited {
            match child.reap_if_idle() {
                Ok(Some(status)) => {
                    self.child = None;
                    self.exit = Some((status, now));
                    return Ok(());
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.terminate(true);
                    return Err(error);
                }
            }
        }

        match self.shutdown {
            None if stop || leader_exited => {
                child.terminate(false)?;
                self.shutdown = Some(Shutdown::Terminating(now + TERMINATE_GRACE));
            }
            Some(Shutdown::Terminating(until)) if now >= until => {
                child.terminate(true)?;
                self.shutdown = Some(Shutdown::Killing(now + KILL_TIMEOUT));
            }
            Some(Shutdown::Killing(until)) if now >= until => {
                child.terminate(true)?;
                self.shutdown = Some(Shutdown::Killing(now + KILL_TIMEOUT));
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the Play process tree did not exit after termination",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn drain(&mut self, emit: &mut impl FnMut(OutputLine)) -> io::Result<()> {
        let stdout = self.stdout.drain(self.exit, emit);
        let stderr = self.stderr.drain(self.exit, emit);
        stdout.and(stderr)
    }

    fn finished(&self) -> Option<ExitStatus> {
        if self.stdout.is_closed() && self.stderr.is_closed() {
            self.exit.map(|(status, _)| status)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
