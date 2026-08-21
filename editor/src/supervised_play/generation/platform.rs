use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
};

#[cfg(windows)]
pub(super) const EXECUTABLE_NAME: &str = "product.exe";
#[cfg(not(windows))]
pub(super) const EXECUTABLE_NAME: &str = "product";

#[cfg(windows)]
pub(super) fn open_directory_handle(path: &Path) -> io::Result<File> {
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
pub(super) fn open_directory_handle(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(windows)]
pub(super) fn open_regular_file(path: &Path) -> io::Result<File> {
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
pub(super) fn open_regular_file(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(windows)]
pub(super) fn artifact_is_executable(path: &Path, _metadata: &fs::Metadata) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

#[cfg(unix)]
pub(super) fn artifact_is_executable(_path: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(any(unix, windows)))]
pub(super) fn artifact_is_executable(_path: &Path, _metadata: &fs::Metadata) -> bool {
    false
}
