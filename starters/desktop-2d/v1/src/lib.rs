use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use nara::fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, FsError, FsOperation,
    HostCapabilityOptions, TrustMode,
};
use nara::project_host::StartupContentClosureReceipt;

const OBSERVATION_ATTEMPT_ENV: &str = "NARA_STARTER_OBSERVATION_ATTEMPT";
const OBSERVATION_FILE_ENV: &str = "NARA_STARTER_OBSERVATION_FILE";

#[doc(hidden)]
#[derive(Clone, Copy)]
pub enum ProductObservationTarget {
    Headless,
    Desktop,
}

impl ProductObservationTarget {
    const fn name(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Desktop => "desktop",
        }
    }

    const fn profile(self) -> &'static str {
        match self {
            Self::Headless => "default",
            Self::Desktop => "desktop",
        }
    }
}

/// Returns the one product recipe shared by the generated targets.
#[must_use]
pub fn product_recipe() -> nara::ProductRecipe {
    nara::ProductRecipe::new()
}

/// Writes one exact startup-closure observation when the parent verification adapter requests it.
#[doc(hidden)]
pub fn record_product_observation(
    target: ProductObservationTarget,
    closure: &StartupContentClosureReceipt,
) -> io::Result<()> {
    let (attempt, output) = match (
        env::var_os(OBSERVATION_ATTEMPT_ENV),
        env::var_os(OBSERVATION_FILE_ENV),
    ) {
        (None, None) => return Ok(()),
        (Some(attempt), Some(output)) => (attempt, output),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "starter observation requires both attempt and output",
            ));
        }
    };
    let attempt = attempt.into_string().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "starter observation attempt must be Unicode",
        )
    })?;
    if attempt.is_empty()
        || attempt.len() > 128
        || !attempt
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "starter observation attempt is invalid",
        ));
    }
    let executable = ContentDigest::of_bytes(&fs::read(env::current_exe()?)?);
    let manifest = closure.manifest_digest();
    let content = closure.content_digest();
    let closure_plan = closure.closure_plan_digest();
    let schema = closure.schema_fingerprint();
    let authoring_bundle = closure
        .members()
        .iter()
        .find(|member| {
            member.kind()
                == nara::project_host::StartupContentMemberKind::AuthoringSchemaBundle
        })
        .map(nara::project_host::StartupContentMemberReceipt::digest)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "starter observation requires the admitted authoring schema bundle",
            )
        })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(b"{")?;
    write!(
        file,
        "\"version\":2,\"attempt\":\"{attempt}\",\"target\":\"{}\",\"profile\":\"{}\",\"executable_length\":{},\"executable_blake3\":\"{}\",\"manifest_length\":{},\"manifest_blake3\":\"{}\",\"content_length\":{},\"content_blake3\":\"{}\",\"closure_plan_length\":{},\"closure_plan_blake3\":\"{}\",\"schema_fingerprint\":\"{}\",\"authoring_bundle_length\":{},\"authoring_bundle_blake3\":\"{}\"",
        target.name(),
        target.profile(),
        executable.length(),
        digest_hex(executable),
        manifest.length(),
        digest_hex(manifest),
        content.length(),
        digest_hex(content),
        closure_plan.length(),
        digest_hex(closure_plan),
        schema,
        authoring_bundle.length(),
        digest_hex(authoring_bundle),
    )?;
    file.write_all(b"}\n")
}

fn digest_hex(digest: ContentDigest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest.as_bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

/// Opens the project source selected by the executable location, never by the process CWD.
///
/// Development binaries use the generated Cargo package root. Exported binaries under `bin/`
/// use the sibling `project/` directory.
pub fn open_project_root() -> Result<DirectoryCapability, FsError> {
    let path = env::current_exe()
        .and_then(|executable| {
            project_root_from_executable(&executable, Path::new(env!("CARGO_MANIFEST_DIR")))
        })
        .map_err(open_directory_error)?;
    let directory = host_directory(&path).map_err(open_directory_error)?;
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
}

fn project_root_from_executable(executable: &Path, development_root: &Path) -> io::Result<PathBuf> {
    let executable_directory = executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "executable has no directory")
        })?;
    let uses_export_layout = executable_directory
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("bin"));
    if !uses_export_layout {
        return Ok(development_root.to_path_buf());
    }
    let export_root = executable_directory
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "export has no root"))?;
    Ok(export_root.join("project"))
}

fn open_directory_error(source: io::Error) -> FsError {
    FsError::Io {
        operation: FsOperation::OpenDirectory,
        source,
    }
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn host_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
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
    {
        File::open(path)
    }
}
