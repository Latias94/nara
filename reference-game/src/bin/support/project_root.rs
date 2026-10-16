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
    project_host::{
        ProductProjectRootError, ProductProjectRootSource, ProjectCandidateError,
        resolve_product_project_root,
    },
};

pub fn open_project_root() -> Result<DirectoryCapability, Box<ProjectCandidateError>> {
    let project_root = env::current_exe()
        .and_then(|executable| default_project_root(&executable, development_project_root()))
        .map_err(project_root_candidate_error)?;
    open_project_root_path(&project_root)
}

pub fn open_explicit_project_root(
    root: &Path,
) -> Result<DirectoryCapability, Box<ProjectCandidateError>> {
    let project_root = resolve_product_project_root(ProductProjectRootSource::Explicit(root))
        .map_err(project_root_error)
        .map_err(project_root_candidate_error)?;
    open_project_root_path(&project_root)
}

fn open_project_root_path(
    project_root: &Path,
) -> Result<DirectoryCapability, Box<ProjectCandidateError>> {
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

fn default_project_root(executable: &Path, development_root: &Path) -> io::Result<PathBuf> {
    match resolve_product_project_root(ProductProjectRootSource::PackagedExecutable(executable)) {
        Ok(root) => Ok(root),
        Err(ProductProjectRootError::UnsupportedPackagedDirectory) => {
            resolve_product_project_root(ProductProjectRootSource::Explicit(development_root))
                .map_err(project_root_error)
        }
        Err(error) => Err(project_root_error(error)),
    }
}

fn development_project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn project_root_error(source: ProductProjectRootError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, source)
}

fn project_root_candidate_error(source: io::Error) -> Box<ProjectCandidateError> {
    Box::new(ProjectCandidateError::from_manifest_authority(
        FsError::Io {
            operation: FsOperation::OpenDirectory,
            source,
        },
    ))
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

    #[cfg(windows)]
    const DEVELOPMENT_ROOT: &str = r"C:\source\reference-game";
    #[cfg(not(windows))]
    const DEVELOPMENT_ROOT: &str = "/source/reference-game";

    #[cfg(windows)]
    const PACKAGED_HEADLESS: &str = r"C:\candidate\nara-reference-game\bin\headless.exe";
    #[cfg(not(windows))]
    const PACKAGED_HEADLESS: &str = "/candidate/nara-reference-game/bin/headless";

    #[cfg(windows)]
    const PACKAGED_DESKTOP: &str = r"C:\candidate\nara-reference-game\bin\desktop.exe";
    #[cfg(not(windows))]
    const PACKAGED_DESKTOP: &str = "/candidate/nara-reference-game/bin/desktop";

    #[cfg(windows)]
    const PACKAGED_PROJECT: &str = r"C:\candidate\nara-reference-game\project";
    #[cfg(not(windows))]
    const PACKAGED_PROJECT: &str = "/candidate/nara-reference-game/project";

    #[cfg(windows)]
    const DEVELOPMENT_EXECUTABLE: &str = r"C:\source\reference-game\target\debug\headless.exe";
    #[cfg(not(windows))]
    const DEVELOPMENT_EXECUTABLE: &str = "/source/reference-game/target/debug/headless";

    #[test]
    fn packaged_product_binaries_use_only_the_sibling_project_directory() {
        let development_root = Path::new(DEVELOPMENT_ROOT);

        for executable in [Path::new(PACKAGED_HEADLESS), Path::new(PACKAGED_DESKTOP)] {
            assert_eq!(
                default_project_root(executable, development_root).unwrap(),
                PathBuf::from(PACKAGED_PROJECT)
            );
        }
    }

    #[test]
    fn cargo_development_binaries_use_the_manifest_root() {
        let development_root = Path::new(DEVELOPMENT_ROOT);

        assert_eq!(
            default_project_root(Path::new(DEVELOPMENT_EXECUTABLE), development_root).unwrap(),
            development_root
        );
    }

    #[test]
    fn supervised_project_root_is_explicit_and_does_not_consult_the_executable() {
        let root = Path::new(DEVELOPMENT_ROOT);

        assert_eq!(
            resolve_product_project_root(ProductProjectRootSource::Explicit(root)).unwrap(),
            root
        );
    }

    #[test]
    fn malformed_packaged_location_is_rejected_instead_of_falling_back() {
        let error =
            default_project_root(Path::new("headless"), Path::new(DEVELOPMENT_ROOT)).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
