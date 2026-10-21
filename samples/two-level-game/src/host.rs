#[cfg(not(windows))]
use std::fs::File;
use std::{
    io,
    path::{Path, PathBuf},
};

use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions,
        TrustMode,
    },
    project_host::{
        ProductProjectRootError, ProductProjectRootSource, resolve_product_project_root,
    },
};

pub fn open_project_root() -> Result<DirectoryCapability, FsError> {
    let executable = std::env::current_exe().map_err(open_error)?;
    let path = match resolve_product_project_root(ProductProjectRootSource::PackagedExecutable(
        &executable,
    )) {
        Ok(path) => path,
        Err(ProductProjectRootError::UnsupportedPackagedDirectory) => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        }
        Err(error) => return Err(open_error(io::Error::other(error))),
    };
    open_explicit_project_root(&path)
}

pub fn open_explicit_project_root(path: &Path) -> Result<DirectoryCapability, FsError> {
    let path = resolve_product_project_root(ProductProjectRootSource::Explicit(path))
        .map_err(|error| open_error(io::Error::other(error)))?;
    directory(&path, CapabilityRights::ReadOnly)
}

pub(crate) fn directory(
    path: &Path,
    rights: CapabilityRights,
) -> Result<DirectoryCapability, FsError> {
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .write(rights == CapabilityRights::ReadWrite)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
    };
    #[cfg(not(windows))]
    let file = File::open(path);
    DirectoryCapability::from_host_handle(
        file.map_err(open_error)?,
        HostCapabilityOptions::new(rights, TrustMode::TrustedLocal),
    )
}

fn open_error(source: io::Error) -> FsError {
    FsError::Io {
        operation: FsOperation::OpenDirectory,
        source,
    }
}
