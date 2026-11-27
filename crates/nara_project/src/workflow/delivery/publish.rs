use std::{io, path::Path};

/// Publishes the complete staged directory without replacing any existing path.
#[cfg(unix)]
pub(super) fn directory(source: &Path, destination: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};

    renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE)?;
    Ok(())
}

#[cfg(windows)]
pub(super) fn directory(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::Storage::FileSystem::MoveFileW;

    let wide = |path: &Path| -> io::Result<Vec<u16>> {
        let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
        if units.contains(&0) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        units.push(0);
        Ok(units)
    };
    let source = wide(source)?;
    let destination = wide(destination)?;
    // Both buffers are NUL-terminated and live until MoveFileW returns.
    if unsafe { MoveFileW(source.as_ptr(), destination.as_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
