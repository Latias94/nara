use std::{
    env,
    fs::File,
    io,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions,
        TrustMode,
    },
    project_host::ProjectCandidateError,
};

pub fn open_project_root() -> Result<DirectoryCapability, Box<ProjectCandidateError>> {
    let project_root = env::current_exe()
        .and_then(|executable| {
            project_root_from_executable(&executable, Path::new(env!("CARGO_MANIFEST_DIR")))
        })
        .map_err(|source| {
            Box::new(ProjectCandidateError::from_manifest_authority(
                FsError::Io {
                    operation: FsOperation::OpenDirectory,
                    source,
                },
            ))
        })?;
    let directory = host_directory(&project_root).map_err(|source| {
        Box::new(ProjectCandidateError::from_manifest_authority(
            FsError::Io {
                operation: FsOperation::OpenDirectory,
                source,
            },
        ))
    })?;
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .map_err(|error| Box::new(ProjectCandidateError::from_manifest_authority(error)))
}

fn project_root_from_executable(executable: &Path, development_root: &Path) -> io::Result<PathBuf> {
    let executable_directory = executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "executable has no directory")
        })?;
    let packaged_directory = executable_directory
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("bin") || name.eq_ignore_ascii_case("tools"));
    if !packaged_directory {
        return Ok(development_root.to_path_buf());
    }
    let package_root = executable_directory
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "package has no root"))?;
    Ok(package_root.join("project"))
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

    #[cfg(unix)]
    {
        File::open(path)
    }

    #[cfg(not(any(windows, unix)))]
    {
        File::open(path)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn packaged_product_binaries_use_only_the_sibling_project_directory() {
        let development_root = Path::new("C:/source/reference-game");

        for executable in [
            Path::new("C:/candidate/nara-reference-game/bin/headless.exe"),
            Path::new("C:/candidate/nara-reference-game/bin/desktop.exe"),
            Path::new("C:/candidate/nara-reference-game/tools/desktop-render-probe.exe"),
        ] {
            assert_eq!(
                project_root_from_executable(executable, development_root).unwrap(),
                PathBuf::from("C:/candidate/nara-reference-game/project")
            );
        }
    }

    #[test]
    fn cargo_development_binaries_use_the_manifest_root() {
        let development_root = Path::new("C:/source/reference-game");

        assert_eq!(
            project_root_from_executable(
                Path::new("C:/source/reference-game/target/debug/headless.exe"),
                development_root,
            )
            .unwrap(),
            development_root
        );
    }

    #[test]
    fn malformed_packaged_location_is_rejected_instead_of_falling_back() {
        let error = project_root_from_executable(
            Path::new("headless.exe"),
            Path::new("C:/source/reference-game"),
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
