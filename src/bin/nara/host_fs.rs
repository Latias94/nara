use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

use nara::fs::{CapabilityRights, DirectoryCapability, FsError, HostCapabilityOptions, TrustMode};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum HostDirectoryError {
    #[error("failed to open the authorized directory")]
    Open(#[source] io::Error),
    #[error("the opened directory cannot satisfy the filesystem capability contract")]
    Capability(#[source] FsError),
}

pub(crate) fn writable_directory(path: &Path) -> Result<DirectoryCapability, HostDirectoryError> {
    let file = open_directory(path).map_err(HostDirectoryError::Open)?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .map_err(HostDirectoryError::Capability)
}

#[cfg(windows)]
fn open_directory(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_directory(path: &Path) -> io::Result<File> {
    File::open(path)
}
