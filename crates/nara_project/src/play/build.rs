use std::{
    env,
    ffi::OsString,
    fs, io,
    num::NonZeroU32,
    path::{self, Path, PathBuf},
    process::ExitStatus,
};

use nara_core::{Diagnostic, DiagnosticSeverity, SourceLoc};
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use super::{OutputLine, OutputStream, ProjectTrust, Run};

/// Selects one binary from a package for an incremental Cargo build for the current host.
#[derive(Debug, Clone, Copy)]
pub struct BuildRequest<'paths> {
    pub project_dir: &'paths Path,
    /// The selected package's manifest within `project_dir`, absolute or relative to that root.
    pub manifest_path: &'paths Path,
    pub package: &'paths str,
    pub binary: &'paths str,
    pub features: &'paths [&'paths str],
    pub release: bool,
}

impl<'paths> BuildRequest<'paths> {
    pub fn new(project_dir: &'paths Path, package: &'paths str, binary: &'paths str) -> Self {
        Self {
            project_dir,
            manifest_path: Path::new("Cargo.toml"),
            package,
            binary,
            features: &[],
            release: false,
        }
    }
}

#[derive(Debug)]
pub enum BuildEvent {
    Output(OutputLine),
    Diagnostic {
        package_id: String,
        diagnostic: BuildDiagnostic,
    },
}

#[derive(Debug, Deserialize)]
pub struct BuildDiagnostic {
    pub level: String,
    pub message: String,
    #[serde(default, deserialize_with = "diagnostic_code")]
    pub code: Option<String>,
    pub rendered: Option<String>,
    pub spans: Vec<DiagnosticSpan>,
    pub children: Vec<BuildDiagnostic>,
}

impl BuildDiagnostic {
    /// Projects Cargo's diagnostic onto the common diagnostics record.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        let severity = match self.level.as_str() {
            "error" | "failure-note" | "error: internal compiler error" => {
                DiagnosticSeverity::Error
            }
            "warning" => DiagnosticSeverity::Warning,
            _ => DiagnosticSeverity::Info,
        };
        let location = self
            .spans
            .iter()
            .find(|span| span.is_primary)
            .map(|span| SourceLoc {
                path: span.file_name.clone(),
                line_column: u32::try_from(span.line_start)
                    .ok()
                    .and_then(NonZeroU32::new)
                    .zip(
                        u32::try_from(span.column_start)
                            .ok()
                            .and_then(NonZeroU32::new),
                    ),
            });
        Diagnostic {
            severity,
            code: self.code.clone(),
            message: self.message.clone(),
            location,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DiagnosticSpan {
    /// The Rust compiler's path, which may be relative to the Cargo workspace.
    pub file_name: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub is_primary: bool,
    pub label: Option<String>,
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("running Cargo requires a trusted project")]
    UntrustedProject,
    #[error("the Cargo manifest must be in the selected project directory")]
    ManifestOutsideProject,
    #[error("could not run the Cargo build: {0}")]
    Io(#[from] io::Error),
    #[error("Cargo build failed ({0})")]
    Failed(ExitStatus),
    #[error("the Cargo build was interrupted")]
    Interrupted,
    #[error("Cargo exited successfully without reporting a successful build completion")]
    Incomplete,
    #[error("Cargo did not report an executable for binary {0:?}")]
    MissingExecutable(String),
    #[error("Cargo reported multiple executable paths for binary {0:?}")]
    AmbiguousExecutable(String),
    #[error("Cargo reported a relative executable path: {0}")]
    RelativeExecutable(PathBuf),
}

/// Runs Cargo in the project directory and delivers diagnostics and text as they arrive. Returns
/// the executable reported for the selected package and binary after a successful build.
pub fn build(
    request: &BuildRequest<'_>,
    trust: ProjectTrust,
    emit: impl FnMut(BuildEvent),
) -> Result<PathBuf, BuildError> {
    build_interruptible(request, trust, || false, emit)
}

/// Runs Cargo like [`build`] and shuts down its process tree when `stop` requests cancellation.
pub fn build_interruptible(
    request: &BuildRequest<'_>,
    trust: ProjectTrust,
    mut stop: impl FnMut() -> bool,
    mut emit: impl FnMut(BuildEvent),
) -> Result<PathBuf, BuildError> {
    if trust != ProjectTrust::Trusted {
        return Err(BuildError::UntrustedProject);
    }
    if stop() {
        return Err(BuildError::Interrupted);
    }
    let directory = request.project_dir.canonicalize()?;
    let manifest = directory.join(request.manifest_path).canonicalize()?;
    if !manifest.starts_with(&directory) {
        return Err(BuildError::ManifestOutsideProject);
    }
    let cargo = cargo_executable()?;
    let mut arguments: Vec<OsString> = [
        "build",
        "--message-format=json",
        "--target=host-tuple",
        "--manifest-path",
    ]
    .map(OsString::from)
    .into();
    arguments.push(manifest.as_os_str().to_owned());
    arguments.push(format!("--package={}", request.package).into());
    arguments.push(format!("--bin={}", request.binary).into());
    if request.release {
        arguments.push("--release".into());
    }
    if !request.features.is_empty() {
        arguments.push(format!("--features={}", request.features.join(",")).into());
    }

    let mut messages = BuildMessages::new(&manifest, request.binary);
    let mut interrupted = false;
    let status = Run::spawn(&directory, &cargo, &arguments)?.wait(
        || {
            interrupted = interrupted || stop();
            interrupted
        },
        &mut |line| messages.push(line, &mut emit),
    )?;
    if interrupted {
        Err(BuildError::Interrupted)
    } else {
        messages.finish(status)
    }
}

#[derive(Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
enum CargoMessage {
    CompilerArtifact {
        manifest_path: PathBuf,
        target: CargoTarget,
        executable: Option<PathBuf>,
    },
    CompilerMessage {
        package_id: String,
        message: BuildDiagnostic,
    },
    BuildFinished {
        success: bool,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
}

struct BuildMessages<'selection> {
    manifest: &'selection Path,
    binary: &'selection str,
    executable: Option<PathBuf>,
    ambiguous: bool,
    successful: bool,
}

impl<'selection> BuildMessages<'selection> {
    fn new(manifest: &'selection Path, binary: &'selection str) -> Self {
        Self {
            manifest,
            binary,
            executable: None,
            ambiguous: false,
            successful: false,
        }
    }

    fn push(&mut self, line: OutputLine, emit: &mut impl FnMut(BuildEvent)) {
        if line.stream == OutputStream::Stderr {
            emit(BuildEvent::Output(line));
            return;
        }
        match serde_json::from_str::<CargoMessage>(&line.text) {
            Ok(CargoMessage::CompilerArtifact {
                manifest_path,
                target,
                executable: Some(executable),
            }) if target.name == self.binary
                && target.kind.iter().any(|kind| kind == "bin")
                && fs::canonicalize(&manifest_path).is_ok_and(|path| path == self.manifest) =>
            {
                if let Some(previous) = &self.executable {
                    self.ambiguous |= *previous != executable;
                } else {
                    self.executable = Some(executable);
                }
            }
            Ok(CargoMessage::CompilerMessage {
                package_id,
                message,
            }) => {
                emit(BuildEvent::Diagnostic {
                    package_id,
                    diagnostic: message,
                });
            }
            Ok(CargoMessage::BuildFinished { success }) => self.successful = success,
            Ok(_) => {}
            Err(_) => emit(BuildEvent::Output(line)),
        }
    }

    fn finish(self, status: ExitStatus) -> Result<PathBuf, BuildError> {
        if !status.success() {
            return Err(BuildError::Failed(status));
        }
        if !self.successful {
            return Err(BuildError::Incomplete);
        }
        if self.ambiguous {
            return Err(BuildError::AmbiguousExecutable(self.binary.to_owned()));
        }
        let executable = self
            .executable
            .ok_or_else(|| BuildError::MissingExecutable(self.binary.to_owned()))?;
        if !executable.is_absolute() {
            return Err(BuildError::RelativeExecutable(executable));
        }
        Ok(executable)
    }
}

fn diagnostic_code<'input, D: Deserializer<'input>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Code {
        code: String,
    }
    Ok(Option::<Code>::deserialize(deserializer)?.map(|code| code.code))
}

fn cargo_executable() -> io::Result<PathBuf> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let path = Path::new(&cargo);
    if path.file_name() != Some(cargo.as_os_str()) {
        return path::absolute(path);
    }
    let mut name = path.to_path_buf();
    if cfg!(windows) && name.extension().is_none() {
        name.set_extension(env::consts::EXE_EXTENSION);
    }
    let search = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&search)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(&name))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("could not find {name:?} on PATH"),
            )
        })
}

#[cfg(test)]
mod tests;
