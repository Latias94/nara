#[cfg(not(windows))]
use std::fs::File;
use std::{io, path::Path};

use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions,
        TrustMode,
    },
    project_host::{open_explicit_product_project_root, open_product_project_root},
};

pub fn open_project_root() -> Result<DirectoryCapability, FsError> {
    open_product_project_root(env!("CARGO_MANIFEST_DIR"))
}

pub fn open_explicit_project_root(path: &Path) -> Result<DirectoryCapability, FsError> {
    open_explicit_product_project_root(path)
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
