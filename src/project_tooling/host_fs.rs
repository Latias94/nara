use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

use crate::fs::{
    CapabilityRights, DirectoryCapability, FileCapability, FsError, HostCapabilityOptions,
    TrustMode,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HostDirectoryError {
    #[error("failed to open the authorized directory")]
    Open(#[source] io::Error),
    #[error("the opened directory cannot satisfy the filesystem capability contract")]
    Capability(#[source] FsError),
}

#[derive(Debug, Error)]
pub enum HostFileError {
    #[error("failed to open the authorized file")]
    Open(#[source] io::Error),
    #[error("the opened file cannot satisfy the filesystem capability contract")]
    Capability(#[source] FsError),
}

pub fn writable_directory(path: &Path) -> Result<DirectoryCapability, HostDirectoryError> {
    directory(path, CapabilityRights::ReadWrite)
}

pub fn readable_directory(path: &Path) -> Result<DirectoryCapability, HostDirectoryError> {
    directory(path, CapabilityRights::ReadOnly)
}

fn directory(
    path: &Path,
    rights: CapabilityRights,
) -> Result<DirectoryCapability, HostDirectoryError> {
    let file = open_directory(path, rights).map_err(HostDirectoryError::Open)?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(rights, TrustMode::TrustedLocal),
    )
    .map_err(HostDirectoryError::Capability)
}

pub fn readable_file(path: &Path) -> Result<FileCapability, HostFileError> {
    let file = open_file(path).map_err(HostFileError::Open)?;
    FileCapability::from_host_handle(file, TrustMode::TrustedLocal, 1)
        .map_err(HostFileError::Capability)
}

#[cfg(windows)]
fn open_directory(path: &Path, rights: CapabilityRights) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(rights == CapabilityRights::ReadWrite)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(path)
}

#[cfg(windows)]
fn open_file(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_directory(path: &Path, _rights: CapabilityRights) -> io::Result<File> {
    File::open(path)
}

#[cfg(not(windows))]
fn open_file(path: &Path) -> io::Result<File> {
    File::open(path)
}
