//! Product project-root selection without current-directory inference.

use std::{
    error::Error,
    fmt,
    fs::File,
    io,
    path::{Component, Path, PathBuf},
};

use nara_fs::{
    CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions, TrustMode,
};

/// Opens the read-only source of an ordinary product without consulting the current directory.
///
/// An executable installed under `bin` or `tools` uses its sibling `project` directory. Other
/// executables use the supplied absolute development root. Failure to open a packaged source
/// never falls back to development data.
#[doc(hidden)]
pub fn open_product_project_root(
    development_root: impl AsRef<Path>,
) -> Result<DirectoryCapability, FsError> {
    let executable = std::env::current_exe().map_err(open_directory_error)?;
    let path = default_project_root(&executable, development_root.as_ref()).map_err(|source| {
        open_directory_error(io::Error::new(io::ErrorKind::InvalidInput, source))
    })?;
    open_explicit_product_project_root(&path)
}

/// Opens one explicit absolute product root with the product Host's read-only source policy.
#[doc(hidden)]
pub fn open_explicit_product_project_root(
    root: impl AsRef<Path>,
) -> Result<DirectoryCapability, FsError> {
    let path = resolve_product_project_root(ProductProjectRootSource::Explicit(root.as_ref()))
        .map_err(|source| {
            open_directory_error(io::Error::new(io::ErrorKind::InvalidInput, source))
        })?;
    open_project_root_path(&path)
}

fn default_project_root(
    executable: &Path,
    development_root: &Path,
) -> Result<PathBuf, ProductProjectRootError> {
    match resolve_product_project_root(ProductProjectRootSource::PackagedExecutable(executable)) {
        Ok(root) => Ok(root),
        Err(ProductProjectRootError::UnsupportedPackagedDirectory) => {
            resolve_product_project_root(ProductProjectRootSource::Explicit(development_root))
        }
        Err(error) => Err(error),
    }
}

fn open_project_root_path(path: &Path) -> Result<DirectoryCapability, FsError> {
    let directory = host_directory(path).map_err(open_directory_error)?;
    let trust = if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    };
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, trust),
    )
}

fn open_directory_error(source: io::Error) -> FsError {
    FsError::Io {
        operation: FsOperation::OpenDirectory,
        source,
    }
}

fn host_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        std::fs::OpenOptions::new()
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

/// Explicit product-root sources accepted by Nara product entry points.
///
/// Development and supervised launches carry an explicit absolute root. Packaged products may
/// derive the sibling `project` directory from an executable installed under `bin` or `tools`.
/// No variant consults the current working directory.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductProjectRootSource<'a> {
    Explicit(&'a Path),
    PackagedExecutable(&'a Path),
}

/// Resolves a product root without consulting the current working directory.
#[doc(hidden)]
pub fn resolve_product_project_root(
    source: ProductProjectRootSource<'_>,
) -> Result<PathBuf, ProductProjectRootError> {
    match source {
        ProductProjectRootSource::Explicit(root) => validate_absolute_path(root, true),
        ProductProjectRootSource::PackagedExecutable(executable) => {
            let executable = validate_absolute_path(executable, false)?;
            let executable_directory = executable
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .ok_or(ProductProjectRootError::ExecutableHasNoDirectory)?;
            let directory_name = executable_directory
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(ProductProjectRootError::UnsupportedPackagedDirectory)?;
            if !directory_name.eq_ignore_ascii_case("bin")
                && !directory_name.eq_ignore_ascii_case("tools")
            {
                return Err(ProductProjectRootError::UnsupportedPackagedDirectory);
            }
            let package_root = executable_directory
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .ok_or(ProductProjectRootError::PackageHasNoRoot)?;
            Ok(package_root.join("project"))
        }
    }
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductProjectRootError {
    PathMustBeAbsolute,
    PathContainsTraversal,
    ExecutableHasNoDirectory,
    UnsupportedPackagedDirectory,
    PackageHasNoRoot,
}

impl fmt::Display for ProductProjectRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PathMustBeAbsolute => "product project-root input must be absolute",
            Self::PathContainsTraversal => {
                "product project-root input cannot contain '.' or '..' components"
            }
            Self::ExecutableHasNoDirectory => "product executable has no containing directory",
            Self::UnsupportedPackagedDirectory => {
                "packaged product executable must be installed under 'bin' or 'tools'"
            }
            Self::PackageHasNoRoot => "packaged product executable has no package root",
        })
    }
}

impl Error for ProductProjectRootError {}

pub(super) fn validate_absolute_path(
    path: &Path,
    allow_root: bool,
) -> Result<PathBuf, ProductProjectRootError> {
    if !path.is_absolute() || (!allow_root && path.parent().is_none()) {
        return Err(ProductProjectRootError::PathMustBeAbsolute);
    }
    if path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(ProductProjectRootError::PathContainsTraversal);
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    const EXPLICIT_PROJECT_ROOT: &str = r"C:\source\game";
    #[cfg(not(windows))]
    const EXPLICIT_PROJECT_ROOT: &str = "/source/game";

    #[cfg(windows)]
    const PACKAGED_EXECUTABLE: &str = r"C:\candidate\game\bin\desktop.exe";
    #[cfg(not(windows))]
    const PACKAGED_EXECUTABLE: &str = "/candidate/game/bin/desktop";

    #[cfg(windows)]
    const PACKAGED_PROJECT_ROOT: &str = r"C:\candidate\game\project";
    #[cfg(not(windows))]
    const PACKAGED_PROJECT_ROOT: &str = "/candidate/game/project";

    #[test]
    fn development_launch_uses_only_the_explicit_project_root() {
        let root = Path::new(EXPLICIT_PROJECT_ROOT);
        assert_eq!(
            resolve_product_project_root(ProductProjectRootSource::Explicit(root)).unwrap(),
            root
        );
    }

    #[test]
    fn packaged_launch_uses_only_the_sibling_project_directory() {
        assert_eq!(
            resolve_product_project_root(ProductProjectRootSource::PackagedExecutable(Path::new(
                PACKAGED_EXECUTABLE,
            )))
            .unwrap(),
            PathBuf::from(PACKAGED_PROJECT_ROOT)
        );
    }

    #[test]
    fn ordinary_launch_selects_packaged_source_before_development_root() {
        assert_eq!(
            default_project_root(
                Path::new(PACKAGED_EXECUTABLE),
                Path::new(EXPLICIT_PROJECT_ROOT),
            )
            .unwrap(),
            PathBuf::from(PACKAGED_PROJECT_ROOT)
        );
        let development_executable = Path::new(EXPLICIT_PROJECT_ROOT)
            .join("target")
            .join("debug")
            .join("desktop");
        assert_eq!(
            default_project_root(&development_executable, Path::new(EXPLICIT_PROJECT_ROOT),)
                .unwrap(),
            PathBuf::from(EXPLICIT_PROJECT_ROOT)
        );
    }

    #[test]
    fn explicit_product_source_is_read_only_and_rejects_relative_paths() {
        let root = std::env::temp_dir().join(format!(
            "nara-product-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("source.txt"), b"product source").unwrap();
        let source = open_explicit_product_project_root(&root).unwrap();
        assert_eq!(source.options().rights(), CapabilityRights::ReadOnly);
        assert!(open_explicit_product_project_root("project").is_err());
        drop(source);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn root_resolution_has_no_relative_or_cwd_fallback() {
        assert_eq!(
            resolve_product_project_root(ProductProjectRootSource::Explicit(Path::new("project"))),
            Err(ProductProjectRootError::PathMustBeAbsolute)
        );
        assert_eq!(
            resolve_product_project_root(ProductProjectRootSource::PackagedExecutable(Path::new(
                "target/debug/desktop",
            ))),
            Err(ProductProjectRootError::PathMustBeAbsolute)
        );
    }
}
