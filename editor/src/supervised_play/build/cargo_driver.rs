use std::{
    env,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use nara::{fs::ContentDigest, project_workflow::__private::CargoDesktopBuildInvocation};
use thiserror::Error;

use super::{
    DesktopBuildFailure, DesktopBuildRequest, MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES,
    MAX_TOOL_IDENTITY_BYTES, MAX_TOOL_PATH_BYTES,
};
use crate::supervised_play::process::SupervisedProcessCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopBuildTool {
    Cargo,
    Rustc,
}

impl DesktopBuildTool {
    const fn environment_override(self) -> &'static str {
        match self {
            Self::Cargo => "CARGO",
            Self::Rustc => "RUSTC",
        }
    }

    const fn binary_stem(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Rustc => "rustc",
        }
    }
}

impl std::fmt::Display for DesktopBuildTool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Cargo => "Cargo",
            Self::Rustc => "rustc",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopBuildTools {
    pub(crate) cargo: PathBuf,
    pub(crate) rustc: PathBuf,
}

impl DesktopBuildTools {
    pub(crate) fn discover() -> Result<Self, DesktopBuildToolError> {
        Ok(Self {
            cargo: discover_tool(DesktopBuildTool::Cargo)?,
            rustc: discover_tool(DesktopBuildTool::Rustc)?,
        })
    }

    #[cfg(test)]
    pub(crate) fn new(
        cargo: impl Into<PathBuf>,
        rustc: impl Into<PathBuf>,
    ) -> Result<Self, DesktopBuildToolError> {
        Ok(Self {
            cargo: admit_tool_path(DesktopBuildTool::Cargo, cargo.into())?,
            rustc: admit_tool_path(DesktopBuildTool::Rustc, rustc.into())?,
        })
    }

    #[cfg(all(test, windows))]
    pub(crate) fn rustc(&self) -> &Path {
        &self.rustc
    }
}

#[derive(Debug, Error)]
pub(crate) enum DesktopBuildToolError {
    #[error("the {tool} environment override must be an absolute executable path")]
    OverrideNotAbsolute { tool: DesktopBuildTool },
    #[error("the {tool} executable could not be found on PATH")]
    NotFound { tool: DesktopBuildTool },
    #[error("the admitted {tool} path is not a regular executable file")]
    NotExecutable { tool: DesktopBuildTool },
    #[error("the admitted {tool} path exceeded its bounded size")]
    PathLimitExceeded { tool: DesktopBuildTool },
    #[error("the admitted {tool} executable could not be canonicalized")]
    Canonicalize {
        tool: DesktopBuildTool,
        #[source]
        source: io::Error,
    },
}

pub(crate) struct ToolIdentityFact {
    pub(crate) text: Box<str>,
    pub(crate) digest: ContentDigest,
}

pub(crate) struct RustcIdentityFact {
    pub(crate) identity: ToolIdentityFact,
    pub(crate) host_target: String,
}

pub(crate) struct BoundedStdout {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedStdout {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<(), BoundedStdoutError> {
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|next| *next <= self.limit)
            .ok_or(BoundedStdoutError::LimitExceeded)?;
        self.bytes
            .try_reserve(next.saturating_sub(self.bytes.len()))
            .map_err(|_| BoundedStdoutError::AllocationFailed)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    pub(crate) fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }

    pub(crate) fn reset(&mut self, limit: usize) {
        self.bytes.clear();
        self.bytes.shrink_to_fit();
        self.limit = limit;
    }
}

#[derive(Debug)]
pub(crate) enum BoundedStdoutError {
    LimitExceeded,
    AllocationFailed,
}

pub(crate) fn cargo_identity_command(request: &DesktopBuildRequest) -> SupervisedProcessCommand {
    SupervisedProcessCommand::new(&request.tools.cargo, &request.project_root)
        .arguments(["--version", "--verbose"])
        .environment("CARGO_TERM_COLOR", "never")
}

pub(crate) fn rustc_identity_command(request: &DesktopBuildRequest) -> SupervisedProcessCommand {
    SupervisedProcessCommand::new(&request.tools.rustc, &request.project_root).argument("-vV")
}

pub(crate) fn metadata_command(request: &DesktopBuildRequest) -> SupervisedProcessCommand {
    SupervisedProcessCommand::new(&request.tools.cargo, &request.project_root)
        .arguments(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .argument("--manifest-path")
        .argument(request.manifest_path.as_os_str())
        .environment("CARGO_TERM_COLOR", "never")
        .environment("RUSTC", request.tools.rustc.as_os_str())
}

pub(crate) fn cargo_build_command(
    request: &DesktopBuildRequest,
    invocation: &CargoDesktopBuildInvocation,
) -> SupervisedProcessCommand {
    SupervisedProcessCommand::new(&request.tools.cargo, &request.project_root)
        .argument("build")
        .argument("--locked")
        .argument("--manifest-path")
        .argument(invocation.manifest_path().as_os_str())
        .argument("--package")
        .argument(invocation.package_name())
        .argument("--bin")
        .argument(invocation.target_name())
        .argument("--profile")
        .argument(invocation.profile())
        .argument("--no-default-features")
        .argument("--features")
        .argument(invocation.features().join(","))
        .argument("--target")
        .argument(invocation.host_target())
        .argument("--message-format=json-diagnostic-rendered-ansi")
        .environment("CARGO_TERM_COLOR", "never")
        .environment(
            "CARGO_TARGET_DIR",
            invocation.target_directory().as_os_str(),
        )
        .environment("RUSTC", request.tools.rustc.as_os_str())
}

pub(crate) fn parse_tool_identity(bytes: &[u8]) -> Option<ToolIdentityFact> {
    if bytes.is_empty() || bytes.len() > MAX_TOOL_IDENTITY_BYTES || bytes.contains(&0) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    if text.trim().is_empty()
        || text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
    {
        return None;
    }
    Some(ToolIdentityFact {
        text: text.trim().to_owned().into_boxed_str(),
        digest: ContentDigest::of_bytes(bytes),
    })
}

pub(crate) fn parse_rustc_identity(bytes: &[u8]) -> Option<RustcIdentityFact> {
    let identity = parse_tool_identity(bytes)?;
    let mut hosts = identity
        .text
        .lines()
        .filter_map(|line| line.strip_prefix("host: "));
    let host_target = hosts.next()?.trim_end_matches('\r').to_owned();
    if hosts.next().is_some()
        || host_target.is_empty()
        || host_target.len() > 256
        || !host_target.is_ascii()
        || !host_target
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    Some(RustcIdentityFact {
        identity,
        host_target,
    })
}

pub(crate) fn digest_build_invocation(invocation: &CargoDesktopBuildInvocation) -> ContentDigest {
    let mut bytes = Vec::with_capacity(256);
    bytes.extend_from_slice(b"nara.desktop-build-invocation.v1\0");
    feed_digest_field(&mut bytes, "package-name", invocation.package_name());
    feed_digest_field(&mut bytes, "package-version", invocation.package_version());
    feed_digest_field(&mut bytes, "target", invocation.target_name());
    feed_digest_field(&mut bytes, "profile", invocation.profile());
    feed_digest_field(&mut bytes, "host-target", invocation.host_target());
    feed_digest_field(&mut bytes, "locked", "true");
    feed_digest_field(&mut bytes, "default-features", "false");
    for feature in invocation.features() {
        feed_digest_field(&mut bytes, "feature", feature);
    }
    ContentDigest::of_bytes(&bytes)
}

fn feed_digest_field(bytes: &mut Vec<u8>, name: &str, value: &str) {
    bytes.extend_from_slice(&(name.len() as u64).to_le_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

pub(crate) fn verify_artifact(path: &Path) -> Result<(), DesktopBuildFailure> {
    let metadata = fs::symlink_metadata(path).map_err(DesktopBuildFailure::ArtifactInspect)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        return Err(DesktopBuildFailure::ArtifactNotExecutable);
    }
    if metadata.len() > MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES {
        return Err(DesktopBuildFailure::ArtifactTooLarge);
    }
    if !platform_file_is_executable(&metadata) {
        return Err(DesktopBuildFailure::ArtifactNotExecutable);
    }
    File::open(path).map_err(DesktopBuildFailure::ArtifactInspect)?;
    Ok(())
}

#[cfg(unix)]
fn platform_file_is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(windows)]
fn platform_file_is_executable(_metadata: &fs::Metadata) -> bool {
    true
}

#[cfg(not(any(unix, windows)))]
fn platform_file_is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

fn discover_tool(tool: DesktopBuildTool) -> Result<PathBuf, DesktopBuildToolError> {
    if let Some(override_path) =
        env::var_os(tool.environment_override()).filter(|value| !value.is_empty())
    {
        let override_path = PathBuf::from(override_path);
        if !override_path.is_absolute() {
            return Err(DesktopBuildToolError::OverrideNotAbsolute { tool });
        }
        return admit_tool_path(tool, override_path);
    }

    let Some(path) = env::var_os("PATH") else {
        return Err(DesktopBuildToolError::NotFound { tool });
    };
    for directory in env::split_paths(&path).filter(|path| !path.as_os_str().is_empty()) {
        for candidate in executable_candidates(&directory, tool.binary_stem()) {
            if candidate.is_file() {
                return admit_tool_path(tool, candidate);
            }
        }
    }
    Err(DesktopBuildToolError::NotFound { tool })
}

fn admit_tool_path(
    tool: DesktopBuildTool,
    path: PathBuf,
) -> Result<PathBuf, DesktopBuildToolError> {
    if !path.is_absolute() {
        return Err(DesktopBuildToolError::OverrideNotAbsolute { tool });
    }
    let file_name = path
        .file_name()
        .ok_or(DesktopBuildToolError::NotExecutable { tool })?;
    let parent = path
        .parent()
        .ok_or(DesktopBuildToolError::NotExecutable { tool })?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|source| DesktopBuildToolError::Canonicalize { tool, source })?;
    // Rustup and other behavior-preserving proxies dispatch from argv[0]. Resolve directory
    // aliases, but retain the admitted leaf name while separately validating its final target.
    let invocation = canonical_parent.join(file_name);
    let resolved = fs::canonicalize(&invocation)
        .map_err(|source| DesktopBuildToolError::Canonicalize { tool, source })?;
    if invocation.as_os_str().as_encoded_bytes().len() > MAX_TOOL_PATH_BYTES
        || resolved.as_os_str().as_encoded_bytes().len() > MAX_TOOL_PATH_BYTES
    {
        return Err(DesktopBuildToolError::PathLimitExceeded { tool });
    }
    let metadata = fs::metadata(&resolved)
        .map_err(|source| DesktopBuildToolError::Canonicalize { tool, source })?;
    if !metadata.is_file() || !platform_file_is_executable(&metadata) {
        return Err(DesktopBuildToolError::NotExecutable { tool });
    }
    Ok(invocation)
}

#[cfg(windows)]
fn executable_candidates(directory: &Path, stem: &str) -> Vec<PathBuf> {
    ["exe", "com"]
        .into_iter()
        .map(|extension| directory.join(format!("{stem}.{extension}")))
        .collect()
}

#[cfg(not(windows))]
fn executable_candidates(directory: &Path, stem: &str) -> Vec<PathBuf> {
    vec![directory.join(stem)]
}
