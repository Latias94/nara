use std::{
    env,
    fs::File,
    io,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::fs::{
    CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions, TrustMode,
};

/// Returns the one product recipe shared by the generated targets.
#[must_use]
pub fn product_recipe() -> nara::ProductRecipe {
    nara::ProductRecipe::new()
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
