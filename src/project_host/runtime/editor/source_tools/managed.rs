//! Explicit Host approval of a framework-dependent captured-image tool.

use std::{ffi::OsString, fmt, sync::Arc, time::Duration};

use nara_fs::{ContentDigest, DirectoryCapability, FsError};

use super::{
    EditorSourceToolCommand, EditorSourceToolDefinition, EditorSourceToolInstallationInspection,
};

mod config;

const BOOTSTRAP: &str = "Nara.SourceToolHost.dll";
const BOOTSTRAP_DEPS: &str = "Nara.SourceToolHost.deps.json";
const BOOTSTRAP_CONFIG: &str = "Nara.SourceToolHost.runtimeconfig.json";

/// Caller-selected byte observations of the trusted launcher and bootstrap distribution.
///
/// These are separate from package-controlled bytes. Approval includes trust in the machine's
/// selected .NET runtime, servicing and OS dependencies; it does not freeze that transitive closure.
#[derive(Debug, Clone, Copy)]
pub struct EditorSourceToolManagedHostDigests {
    pub launcher: ContentDigest,
    pub bootstrap: ContentDigest,
    pub deps: ContentDigest,
    pub runtimeconfig: ContentDigest,
}

#[derive(Debug, thiserror::Error)]
pub enum EditorSourceToolManagedError {
    #[error("captured managed source tools are unsupported on this Host")]
    Unsupported,
    #[error("the selected managed tool contract is unsupported: {0}")]
    Contract(&'static str),
    #[error("managed Host filesystem admission failed for {member}")]
    File {
        member: &'static str,
        #[source]
        source: FsError,
    },
    #[error("managed Host native image admission failed")]
    Image(#[source] std::io::Error),
    #[error("the selected source-tool declaration is invalid")]
    Definition(#[source] nara_tooling::EditorSourceToolError),
}

/// An explicitly approved trusted machine prerequisite, prepared without executing package code.
///
/// Use a Host-selected runtime root and bootstrap distribution, never capabilities recovered from
/// package declarations. The first supported platform is Windows x64 MSVC. This owner admits the
/// launcher/bootstrap bytes; the runtime's framework, servicing and system dependencies remain an
/// explicit trusted machine baseline rather than an immutable dependency snapshot.
#[derive(Clone)]
pub struct EditorSourceToolManagedHost {
    #[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
    custody: Arc<HostCustody>,
}

impl fmt::Debug for EditorSourceToolManagedHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorSourceToolManagedHost")
            .finish_non_exhaustive()
    }
}

impl EditorSourceToolManagedHost {
    /// Explicitly approves the selected .NET 10 machine runtime and known bootstrap policy.
    ///
    /// Select an exact canonical `10.0.patch` framework version. Startup uses an empty environment
    /// with only Windows session/temp facts and fixed diagnostic/store settings. This operation
    /// performs bounded IO and should run in Host preparation, never while painting.
    pub fn new(
        runtime: DirectoryCapability,
        bootstrap: DirectoryCapability,
        expected: EditorSourceToolManagedHostDigests,
        framework_version: &str,
        timeout: Duration,
    ) -> Result<Self, EditorSourceToolManagedError> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
        {
            Ok(Self {
                custody: Arc::new(HostCustody::prepare(
                    runtime,
                    bootstrap,
                    expected,
                    framework_version,
                    timeout,
                )?),
            })
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")))]
        {
            let _ = (runtime, bootstrap, expected, framework_version, timeout);
            Err(EditorSourceToolManagedError::Unsupported)
        }
    }

    /// Explicitly approves this inspected installation's captured managed images for Session use.
    ///
    /// The returned definition still needs the ordinary Session describe probe before activation.
    /// This neither registers project metadata nor persists enablement. Required Package
    /// relationships reject until the installed-graph owner can validate them. The namespace is
    /// not consulted again for package execution: each invocation streams these same owned bytes.
    pub fn approve_installation(
        &self,
        installation: EditorSourceToolInstallationInspection,
    ) -> Result<EditorSourceToolDefinition, EditorSourceToolManagedError> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
        {
            let release = installation.release();
            let prefix = config::prepare_images(release)?;
            let mut command = self.custody.command.clone();
            command.approved = Some(Arc::new(ApprovedManagedLaunch {
                host: Arc::clone(&self.custody),
                prefix: prefix.into(),
                provenance: ApprovedInstallationProvenance {
                    directory: installation.directory().share(),
                    package: Arc::from(release.package()),
                    installation: *installation.receipt().installation_id(),
                    receipt: installation.receipt_digest(),
                    manifest: release.manifest_digest(),
                },
            }));
            EditorSourceToolDefinition::new(
                command,
                release.schema_owner().clone(),
                release.component().clone(),
            )
            .map_err(EditorSourceToolManagedError::Definition)
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")))]
        {
            let _ = installation;
            Err(EditorSourceToolManagedError::Unsupported)
        }
    }
}

pub(super) struct ApprovedManagedLaunch {
    #[cfg(windows)]
    host: Arc<HostCustody>,
    prefix: Arc<[u8]>,
    provenance: ApprovedInstallationProvenance,
}

/// Immutable inspected installation facts, separate from Host and captured-image custody.
struct ApprovedInstallationProvenance {
    directory: DirectoryCapability,
    package: Arc<str>,
    installation: [u8; 32],
    receipt: ContentDigest,
    manifest: ContentDigest,
}

impl fmt::Debug for ApprovedManagedLaunch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApprovedManagedLaunch")
            .field("package", &self.provenance.package)
            .field("installation", &self.provenance.installation)
            .field("directory_identity", &self.provenance.directory.identity())
            .field("receipt_digest", &self.provenance.receipt)
            .field("manifest", &self.provenance.manifest)
            .field("input_bytes", &self.prefix.len())
            .finish_non_exhaustive()
    }
}

impl ApprovedManagedLaunch {
    /// Borrows the approved directory, package, installation ID, receipt and manifest digests.
    pub(super) fn installation_facts(
        &self,
    ) -> (
        &DirectoryCapability,
        &str,
        &[u8; 32],
        ContentDigest,
        ContentDigest,
    ) {
        (
            &self.provenance.directory,
            &self.provenance.package,
            &self.provenance.installation,
            self.provenance.receipt,
            self.provenance.manifest,
        )
    }

    pub(super) fn prefix(&self) -> &[u8] {
        &self.prefix
    }

    #[cfg(windows)]
    pub(super) fn image(&self) -> &crate::project_host::process_image::ExactProcessImage {
        &self.host.image
    }

    pub(super) fn environment(&self) -> &[(OsString, OsString)] {
        #[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
        {
            &self.host.environment
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")))]
        {
            &[]
        }
    }

    pub(super) fn verify(&self) -> Result<(), FsError> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
        {
            self.host.verify()
        }
        #[cfg(not(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")))]
        {
            Err(FsError::Unsupported {
                operation: nara_fs::FsOperation::CloneHandle,
                capability: "captured managed tool launch",
            })
        }
    }
}

#[cfg(windows)]
struct HostCustody {
    command: EditorSourceToolCommand,
    image: crate::project_host::process_image::ExactProcessImage,
    _bootstrap_directory: nara_fs::RestrictiveCapabilityHandleLease,
    bootstrap: DirectoryCapability,
    members: Vec<HostMember>,
    environment: Vec<(OsString, OsString)>,
}

#[cfg(windows)]
struct HostMember {
    path: &'static str,
    expected: ContentDigest,
    file: nara_fs::FileCapability,
    _guard: nara_fs::RestrictiveCapabilityHandleLease,
}

#[cfg(windows)]
impl HostCustody {
    fn prepare(
        runtime: DirectoryCapability,
        bootstrap: DirectoryCapability,
        expected: EditorSourceToolManagedHostDigests,
        version: &str,
        timeout: Duration,
    ) -> Result<Self, EditorSourceToolManagedError> {
        use crate::project_host::process_image::{ExactProcessImage, capability_path};
        use nara_fs::{RelativeComponent, TrustMode};
        if !matches!(
            runtime.options().trust(),
            TrustMode::Untrusted | TrustMode::Recovery
        ) || !matches!(
            bootstrap.options().trust(),
            TrustMode::Untrusted | TrustMode::Recovery
        ) {
            return Err(EditorSourceToolManagedError::Contract(
                "strict Host capabilities required",
            ));
        }
        let patch = version
            .strip_prefix("10.0.")
            .and_then(|patch| patch.parse::<u32>().ok());
        if !patch.is_some_and(|patch| format!("10.0.{patch}") == version) {
            return Err(EditorSourceToolManagedError::Contract(
                "exact .NET 10 framework version required",
            ));
        }
        let file_error = |member, source| EditorSourceToolManagedError::File { member, source };
        let launcher = runtime
            .open_child_file(&RelativeComponent::new("dotnet.exe").expect("fixed launcher name"))
            .map_err(|source| file_error("dotnet.exe", source))?;
        let image = ExactProcessImage::from_verified_handles(
            launcher
                .duplicate_restrictive_executable_host_handle()
                .map_err(|source| file_error("dotnet.exe", source))?,
            runtime
                .duplicate_restrictive_host_handle()
                .map_err(|source| file_error("runtime directory", source))?,
        )
        .map_err(EditorSourceToolManagedError::Image)?;
        if expected.launcher.length() == 0
            || expected.launcher.length() > 128 * 1024 * 1024
            || launcher
                .digest(expected.launcher.length())
                .map_err(|source| file_error("dotnet.exe", source))?
                != expected.launcher
        {
            return Err(EditorSourceToolManagedError::Contract(
                "launcher digest mismatch",
            ));
        }
        let directory = bootstrap
            .duplicate_restrictive_host_handle()
            .map_err(|source| file_error("bootstrap directory", source))?;
        let directory_path =
            capability_path(&directory).map_err(EditorSourceToolManagedError::Image)?;
        let mut members = Vec::with_capacity(3);
        for (path, digest, limit) in [
            (BOOTSTRAP, expected.bootstrap, 16 * 1024 * 1024),
            (BOOTSTRAP_DEPS, expected.deps, 64 * 1024),
            (BOOTSTRAP_CONFIG, expected.runtimeconfig, 64 * 1024),
        ] {
            if digest.length() == 0 || digest.length() > limit {
                return Err(EditorSourceToolManagedError::Contract(
                    "bootstrap member byte limit",
                ));
            }
            let file = bootstrap
                .open_child_file(&RelativeComponent::new(path).expect("fixed bootstrap member"))
                .map_err(|source| file_error(path, source))?;
            let guard = file
                .duplicate_restrictive_host_handle()
                .map_err(|source| file_error(path, source))?;
            if file
                .digest(digest.length())
                .map_err(|source| file_error(path, source))?
                != digest
            {
                return Err(EditorSourceToolManagedError::Contract(
                    "bootstrap digest mismatch",
                ));
            }
            members.push(HostMember {
                path,
                expected: digest,
                file,
                _guard: guard,
            });
        }
        let command = EditorSourceToolCommand::new(
            image.path().to_owned(),
            vec![
                "exec".into(),
                "--fx-version".into(),
                version.into(),
                "--roll-forward".into(),
                "Disable".into(),
                "--depsfile".into(),
                directory_path.join(BOOTSTRAP_DEPS).into_os_string(),
                "--runtimeconfig".into(),
                directory_path.join(BOOTSTRAP_CONFIG).into_os_string(),
                directory_path.join(BOOTSTRAP).into_os_string(),
            ],
            directory_path,
            timeout,
        )
        .map_err(EditorSourceToolManagedError::Definition)?;
        let mut environment: Vec<_> = ["SystemRoot", "WINDIR", "TEMP", "TMP"]
            .into_iter()
            .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
            .collect();
        environment.extend([
            ("DOTNET_EnableDiagnostics".into(), "0".into()),
            ("DOTNET_MULTILEVEL_LOOKUP".into(), "0".into()),
        ]);
        Ok(Self {
            command,
            image,
            _bootstrap_directory: directory,
            bootstrap,
            members,
            environment,
        })
    }

    fn verify(&self) -> Result<(), FsError> {
        use nara_fs::RelativePath;
        for member in &self.members {
            let current = self.bootstrap.open_file_exact(
                &RelativePath::new(member.path).expect("fixed bootstrap member"),
            )?;
            if current.identity() != member.file.identity()
                || current.digest(member.expected.length())? != member.expected
            {
                return Err(FsError::CapabilityIdentityDrift {
                    operation: nara_fs::FsOperation::CloneHandle,
                });
            }
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    target_os = "windows",
    target_arch = "x86_64",
    target_env = "msvc"
))]
mod tests;
