use std::{
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
};

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

fn validate_absolute_path(
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
