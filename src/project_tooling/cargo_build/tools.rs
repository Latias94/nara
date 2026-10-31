//! Selected native tools and their revalidated path/object/content observations.

use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use nara_fs::{ContentDigest, FileCapability, FsError};

use super::{CargoBuildFailure, CargoToolchainObservation, bounded_output, parse_host_target};
use crate::project_tooling::host_fs;

const MAX_RESOLVED_TOOL_PATH_BYTES: usize = 128 * 1024;

/// Actual Cargo and rustc executables selected by a trusted Host.
///
/// Rustup proxies are resolved once in the selected workspace. Subsequent operations use the
/// actual toolchain paths, independently of later rustup override or default-toolchain changes.
/// Multiply linked candidates without an identifiable rustup manager are rejected, including
/// genuine tools installed with that ambiguous layout.
#[derive(Debug, Clone)]
pub struct CargoBuildTools {
    cargo: PathBuf,
    rustc: PathBuf,
}

impl CargoBuildTools {
    /// Admits absolute tool paths, resolving rustup proxies in the current directory.
    pub fn new(
        cargo: impl Into<PathBuf>,
        rustc: impl Into<PathBuf>,
    ) -> Result<Self, CargoBuildFailure> {
        let workspace = env::current_dir().map_err(CargoBuildFailure::Io)?;
        Self::resolve(cargo.into(), rustc.into(), &workspace)
    }

    /// Discovers tools in the current directory's rustup context.
    pub fn discover() -> Result<Self, CargoBuildFailure> {
        let workspace = env::current_dir().map_err(CargoBuildFailure::Io)?;
        Self::discover_in(&workspace)
    }

    /// Discovers tools in an explicit workspace's rustup context without acquiring a toolchain.
    ///
    /// Absolute `CARGO` and `RUSTC` overrides take precedence over absolute `PATH` entries.
    pub fn discover_in(workspace: &Path) -> Result<Self, CargoBuildFailure> {
        let workspace = std::fs::canonicalize(workspace).map_err(CargoBuildFailure::Io)?;
        if !workspace.is_dir() {
            return Err(CargoBuildFailure::InvalidTool);
        }
        Self::resolve(
            discover_tool("CARGO", "cargo")?,
            discover_tool("RUSTC", "rustc")?,
            &workspace,
        )
    }

    fn resolve(
        cargo: PathBuf,
        rustc: PathBuf,
        workspace: &Path,
    ) -> Result<Self, CargoBuildFailure> {
        if !cargo.is_absolute() || !rustc.is_absolute() {
            return Err(CargoBuildFailure::InvalidTool);
        }
        Ok(Self {
            cargo: resolve_tool(cargo, "cargo", workspace)?,
            rustc: resolve_tool(rustc, "rustc", workspace)?,
        })
    }

    pub(crate) fn cargo_path(&self) -> &Path {
        &self.cargo
    }

    pub(crate) fn rustc_path(&self) -> &Path {
        &self.rustc
    }
}

/// Continuously held tool files plus their observed implementation and version facts.
///
/// Verification proves the declared before/after path/object/content envelope. It does not attest
/// transitive native dependencies or prevent a cooperative writer from changing and restoring a
/// file entirely between observations.
#[derive(Debug)]
pub(super) struct ToolObservation {
    cargo: FileCapability,
    rustc: FileCapability,
    facts: CargoToolchainObservation,
}

impl ToolObservation {
    /// Rechecks that the selected names still identify the same files and exact observed bytes.
    pub fn verify(&self, tools: &CargoBuildTools) -> Result<(), CargoBuildFailure> {
        verify_tool(tools.cargo_path(), &self.cargo, self.facts.cargo)?;
        verify_tool(tools.rustc_path(), &self.rustc, self.facts.rustc)
    }

    #[must_use]
    pub fn facts(&self) -> &CargoToolchainObservation {
        &self.facts
    }

    fn version_output(
        &self,
        tools: &CargoBuildTools,
        workspace: &Path,
        path: &Path,
        arguments: &[&str],
        identity_limit: usize,
    ) -> Result<Vec<u8>, CargoBuildFailure> {
        self.verify(tools)?;
        let mut command = Command::new(path);
        command
            .current_dir(workspace)
            .args(arguments)
            .env("RUSTC", tools.rustc_path())
            .env("CARGO_TERM_COLOR", "never")
            .env("RUSTUP_AUTO_INSTALL", "0")
            .stdin(Stdio::null());
        let output = bounded_output(&mut command, identity_limit);
        let verified = self.verify(tools);
        // Preserve a process error's possible cleanup custody even if binding also failed.
        let output = output?;
        verified?;
        Ok(output)
    }
}

/// Observes actual tool files and versions, checking both bindings around each version process.
pub(super) fn observe_tools(
    tools: &CargoBuildTools,
    workspace: &Path,
    identity_limit: usize,
    tool_byte_bound: u64,
) -> Result<ToolObservation, CargoBuildFailure> {
    let cargo = host_fs::readable_file(tools.cargo_path())?;
    let rustc = host_fs::readable_file(tools.rustc_path())?;
    let cargo_digest = cargo.digest(tool_byte_bound)?;
    let rustc_digest = rustc.digest(tool_byte_bound)?;
    if cargo_digest.length() == 0 || rustc_digest.length() == 0 {
        return Err(CargoBuildFailure::InvalidTool);
    }
    let mut observed = ToolObservation {
        cargo,
        rustc,
        facts: CargoToolchainObservation {
            cargo: cargo_digest,
            rustc: rustc_digest,
            cargo_version: ContentDigest::of_bytes(&[]),
            rustc_version: ContentDigest::of_bytes(&[]),
            host_target: String::new(),
        },
    };
    observed.facts.cargo_version = {
        let output = observed.version_output(
            tools,
            workspace,
            tools.cargo_path(),
            &["--version", "--verbose"],
            identity_limit,
        )?;
        version_digest(&output, "cargo ")?
    };
    {
        let output = observed.version_output(
            tools,
            workspace,
            tools.rustc_path(),
            &["-vV"],
            identity_limit,
        )?;
        observed.facts.rustc_version = version_digest(&output, "rustc ")?;
        observed.facts.host_target = parse_host_target(&output)?;
    }
    Ok(observed)
}

fn verify_tool(
    path: &Path,
    observed: &FileCapability,
    digest: ContentDigest,
) -> Result<(), CargoBuildFailure> {
    let current = host_fs::readable_file(path)?;
    if !current.same_object(observed)? {
        return Err(CargoBuildFailure::Stale);
    }
    current
        .verify_digest(digest, digest.length())
        .map_err(|error| match error {
            FsError::DigestMismatch { .. } | FsError::ByteLimitExceeded { .. } => {
                CargoBuildFailure::Stale
            }
            error => CargoBuildFailure::Filesystem(error),
        })
}

fn version_digest(bytes: &[u8], prefix: &str) -> Result<ContentDigest, CargoBuildFailure> {
    let text = std::str::from_utf8(bytes).map_err(|_| CargoBuildFailure::InvalidToolIdentity)?;
    if !text
        .lines()
        .next()
        .is_some_and(|line| line.starts_with(prefix))
    {
        return Err(CargoBuildFailure::InvalidToolIdentity);
    }
    Ok(ContentDigest::of_bytes(bytes))
}

fn discover_tool(variable: &str, stem: &str) -> Result<PathBuf, CargoBuildFailure> {
    if let Some(path) = env::var_os(variable) {
        let path = PathBuf::from(path);
        return if path.is_absolute() {
            Ok(path)
        } else {
            Err(CargoBuildFailure::InvalidTool)
        };
    }
    let path = env::var_os("PATH").ok_or(CargoBuildFailure::InvalidTool)?;
    let name = executable_name(stem);
    for directory in env::split_paths(&path).filter(|directory| directory.is_absolute()) {
        let candidate = directory.join(&name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(CargoBuildFailure::InvalidTool)
}

fn resolve_tool(
    selected: PathBuf,
    stem: &str,
    workspace: &Path,
) -> Result<PathBuf, CargoBuildFailure> {
    let actual = canonical_executable(&selected)?;
    if has_tool_name(&actual, "rustup") {
        return resolve_rustup(&actual, stem, workspace);
    }
    if !has_tool_name(&actual, stem) {
        return Err(CargoBuildFailure::InvalidTool);
    }
    let file = host_fs::readable_file(&actual)?;
    match rustup_proxy(&selected, &actual, &file)? {
        Some(rustup) => resolve_rustup(&rustup, stem, workspace),
        None if file.link_count() > 1 => Err(CargoBuildFailure::InvalidTool),
        None => Ok(actual),
    }
}

fn rustup_proxy(
    selected: &Path,
    actual: &Path,
    file: &FileCapability,
) -> Result<Option<PathBuf>, CargoBuildFailure> {
    let name = executable_name("rustup");
    for parent in [selected.parent(), actual.parent()].into_iter().flatten() {
        if let Some(rustup) = same_rustup_file(&parent.join(&name), file)? {
            return Ok(Some(rustup));
        }
    }
    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path).filter(|directory| directory.is_absolute()) {
            if let Some(rustup) = same_rustup_file(&directory.join(&name), file)? {
                return Ok(Some(rustup));
            }
        }
    }
    Ok(None)
}

fn same_rustup_file(
    candidate: &Path,
    selected: &FileCapability,
) -> Result<Option<PathBuf>, CargoBuildFailure> {
    if !candidate.is_file() {
        return Ok(None);
    }
    let actual = canonical_executable(candidate)?;
    let rustup = host_fs::readable_file(&actual)?;
    if !rustup.same_object(selected)? {
        return Ok(None);
    }
    if !has_tool_name(&actual, "rustup") {
        return Err(CargoBuildFailure::InvalidTool);
    }
    Ok(Some(actual))
}

fn resolve_rustup(
    rustup: &Path,
    stem: &str,
    workspace: &Path,
) -> Result<PathBuf, CargoBuildFailure> {
    let manager = host_fs::readable_file(rustup)?;
    let mut command = Command::new(rustup);
    command
        .current_dir(workspace)
        .args(["which", stem])
        .env("RUSTUP_AUTO_INSTALL", "0")
        .stdin(Stdio::null());
    let output = bounded_output(&mut command, MAX_RESOLVED_TOOL_PATH_BYTES);
    let verified = (|| {
        let current = host_fs::readable_file(rustup)?;
        if !current.same_object(&manager)? {
            return Err(CargoBuildFailure::Stale);
        }
        Ok::<(), CargoBuildFailure>(())
    })();
    let output = output?;
    verified?;
    let text = std::str::from_utf8(&output).map_err(|_| CargoBuildFailure::InvalidToolIdentity)?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    if text.is_empty() || text.contains(['\r', '\n', '\0']) {
        return Err(CargoBuildFailure::InvalidToolIdentity);
    }
    let selected = PathBuf::from(text);
    if !selected.is_absolute() || !has_tool_name(&selected, stem) {
        return Err(CargoBuildFailure::InvalidTool);
    }
    let actual = canonical_executable(&selected)?;
    if !has_tool_name(&actual, stem) {
        return Err(CargoBuildFailure::InvalidTool);
    }
    let file = host_fs::readable_file(&actual)?;
    if file.same_object(&manager)?
        || rustup_proxy(&selected, &actual, &file)?.is_some()
        || file.link_count() > 1
    {
        return Err(CargoBuildFailure::InvalidTool);
    }
    Ok(actual)
}

fn canonical_executable(path: &Path) -> Result<PathBuf, CargoBuildFailure> {
    if !path.is_absolute() {
        return Err(CargoBuildFailure::InvalidTool);
    }
    let actual = std::fs::canonicalize(path).map_err(CargoBuildFailure::Io)?;
    let metadata = std::fs::metadata(&actual).map_err(CargoBuildFailure::Io)?;
    if !metadata.is_file() {
        return Err(CargoBuildFailure::InvalidTool);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(CargoBuildFailure::InvalidTool);
        }
    }
    Ok(actual)
}

fn executable_name(stem: &str) -> String {
    format!("{stem}{}", env::consts::EXE_SUFFIX)
}

fn has_tool_name(path: &Path, stem: &str) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let expected = executable_name(stem);
    #[cfg(windows)]
    {
        name.eq_ignore_ascii_case(&expected)
    }
    #[cfg(not(windows))]
    {
        name == expected
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn unresolved_hard_link_is_rejected_without_executing_the_candidate() {
        let root = env::temp_dir().join(format!(
            "nara-orphan-tool-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_file(self.0.join(executable_name("cargo")));
                let _ = fs::remove_file(self.0.join("tool-hardlink"));
                let _ = fs::remove_dir(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let candidate = root.join(executable_name("cargo"));
        let alias = root.join("tool-hardlink");
        // This is deliberately not executable code: resolution must never launch the candidate.
        fs::write(&candidate, b"tool identity fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::hard_link(&candidate, &alias).unwrap();
        {
            let original = host_fs::readable_file(&candidate).unwrap();
            let linked = host_fs::readable_file(&alias).unwrap();
            assert!(original.same_object(&linked).unwrap());
            assert!(original.link_count() > 1);
        }
        assert!(matches!(
            resolve_tool(candidate.clone(), "cargo", &root),
            Err(CargoBuildFailure::InvalidTool),
        ));

        fs::remove_file(&alias).unwrap();
        assert_eq!(
            resolve_tool(candidate.clone(), "cargo", &root).unwrap(),
            candidate.canonicalize().unwrap(),
        );
    }
}
