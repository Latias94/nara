use std::{
    env,
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use nara::fs::{
    CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions, TrustMode,
};
use nara::project_host::{
    ProductProjectRootError, ProductProjectRootSource, resolve_product_project_root,
};

#[doc(hidden)]
pub use nara::project_tooling::{ProductObservationTarget, record_product_observation};

/// Returns the one product recipe shared by the generated targets.
#[must_use]
pub fn product_recipe() -> nara::ProductRecipe {
    let recipe = nara::ProductRecipe::new()
        .add_contribution(collect_gameplay::contribution().expect("the game schema is valid"))
        .expect("the game recipe is statically valid");
    #[cfg(feature = "desktop")]
    let recipe = recipe
        .add_plugin::<collect_gameplay::DesktopInputPlugin>()
        .expect("the desktop input recipe is statically valid");
    recipe
}

/// Opens the ordinary project source without consulting the process CWD.
///
/// Development binaries use the explicit generated Cargo package root. Exported binaries under
/// `bin/` use the sibling `project/` directory. Supervised desktop launches use
/// [`open_explicit_project_root`] with the root carried by Nara's inherited launch context.
pub fn open_project_root() -> Result<DirectoryCapability, FsError> {
    let path = env::current_exe()
        .and_then(|executable| default_project_root(&executable, development_project_root()))
        .map_err(open_directory_error)?;
    open_project_root_path(&path)
}

/// Opens the explicit absolute project root carried by a supervised desktop launch context.
#[doc(hidden)]
pub fn open_explicit_project_root(root: &Path) -> Result<DirectoryCapability, FsError> {
    let path = resolve_product_project_root(ProductProjectRootSource::Explicit(root))
        .map_err(project_root_error)
        .map_err(open_directory_error)?;
    open_project_root_path(&path)
}

fn open_project_root_path(path: &Path) -> Result<DirectoryCapability, FsError> {
    let directory = host_directory(&path).map_err(open_directory_error)?;
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
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
