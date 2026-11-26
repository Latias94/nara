use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, ensure};
use nara_core::vfs::write_atomic;

pub(super) const MAX_LAYOUT_BYTES: usize = 1024 * 1024;

pub(super) fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    (|| -> Result<_> {
        let checked = match checked_path(path, false) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let metadata = match fs::symlink_metadata(&checked) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        check_file(&metadata)?;
        let file = open_layout(&checked)?;
        check_file(&file.metadata()?)?;
        let mut bytes = Vec::new();
        file.take(MAX_LAYOUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        check_size(bytes.len())?;
        Ok(Some(bytes))
    })()
    .with_context(|| format!("Could not read editor layout at {}", path.display()))
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    (|| -> Result<()> {
        check_size(bytes.len())?;
        let checked = checked_path(path, true)?;
        match fs::symlink_metadata(&checked) {
            Ok(metadata) => check_file(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        write_atomic(&checked, bytes)?;
        Ok(())
    })()
    .with_context(|| format!("Could not write editor layout at {}", path.display()))
}

fn checked_path(path: &Path, create: bool) -> io::Result<PathBuf> {
    let directory = path
        .parent()
        .ok_or_else(|| invalid("missing layout directory"))?;
    let directory_name = directory
        .file_name()
        .ok_or_else(|| invalid("missing layout directory name"))?;
    let root = directory
        .parent()
        .filter(|root| !root.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let directory = root.join(directory_name);
    if create {
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let metadata = fs::symlink_metadata(&directory)?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(invalid("layout directory must be a real project directory"));
    }
    let directory = directory.canonicalize()?;
    if directory.parent() != Some(root.as_path()) {
        return Err(invalid(
            "layout directory must be directly inside the project directory",
        ));
    }
    let name = path
        .file_name()
        .ok_or_else(|| invalid("missing layout filename"))?;
    Ok(directory.join(name))
}

fn check_file(metadata: &Metadata) -> io::Result<()> {
    if !metadata.is_file() || is_link(metadata) {
        return Err(invalid(
            "layout must be a regular file without symlinks or reparse points",
        ));
    }
    Ok(())
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn open_layout(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        #[cfg(target_os = "macos")]
        const O_NOFOLLOW: i32 = 0x0100;
        #[cfg(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "arm")))]
        const O_NOFOLLOW: i32 = 0o100_000;
        #[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "x86")))]
        const O_NOFOLLOW: i32 = 0o400_000;
        #[cfg(any(
            target_os = "macos",
            all(
                target_os = "linux",
                any(
                    target_arch = "x86_64",
                    target_arch = "x86",
                    target_arch = "aarch64",
                    target_arch = "arm"
                )
            )
        ))]
        options.custom_flags(O_NOFOLLOW);
    }
    options.open(path)
}

fn check_size(len: usize) -> Result<()> {
    ensure!(
        len <= MAX_LAYOUT_BYTES,
        "layout exceeds the {MAX_LAYOUT_BYTES}-byte limit"
    );
    Ok(())
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
