use std::{io, path::PathBuf};

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
use std::fs;

const STORE_ROOT_NAME: &str = "nara-editor-supervised-play";

pub(super) fn generation_store_base_root() -> io::Result<PathBuf> {
    platform::generation_store_base_root()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod platform {
    use std::{
        env,
        ffi::OsStr,
        fs::{self, DirBuilder},
        io,
        os::unix::fs::MetadataExt as _,
        path::{Path, PathBuf},
    };

    use super::STORE_ROOT_NAME;

    const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
    const STICKY_BIT: u32 = 0o1_000;

    pub(super) fn generation_store_base_root() -> io::Result<PathBuf> {
        let effective_uid = rustix::process::geteuid().as_raw();
        if let Some(runtime_directory) = env::var_os("XDG_RUNTIME_DIR") {
            let runtime_directory = PathBuf::from(runtime_directory);
            validate_runtime_directory(&runtime_directory, effective_uid)?;
            let root = runtime_directory.join(STORE_ROOT_NAME);
            create_and_validate_private_directory(&root, effective_uid)?;
            return Ok(root);
        }

        let temporary_directory = Path::new("/tmp");
        validate_sticky_temporary_directory(temporary_directory)?;
        let root = temporary_directory.join(format!("{STORE_ROOT_NAME}-{effective_uid}"));
        create_and_validate_private_directory(&root, effective_uid)?;
        Ok(root)
    }

    fn create_and_validate_private_directory(path: &Path, effective_uid: u32) -> io::Result<()> {
        match DirBuilder::new().mode(PRIVATE_DIRECTORY_MODE).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        validate_private_directory(path, effective_uid)
    }

    fn validate_runtime_directory(path: &Path, effective_uid: u32) -> io::Result<()> {
        if !path.is_absolute() {
            return Err(invalid_directory(
                path.as_os_str(),
                "XDG_RUNTIME_DIR is not absolute",
            ));
        }
        validate_private_directory(path, effective_uid)
    }

    fn validate_private_directory(path: &Path, effective_uid: u32) -> io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid_directory(
                path.as_os_str(),
                "the path is not a physical directory",
            ));
        }
        if metadata.uid() != effective_uid {
            return Err(invalid_directory(
                path.as_os_str(),
                "the directory is not owned by the effective user",
            ));
        }
        if metadata.mode() & 0o777 != PRIVATE_DIRECTORY_MODE {
            return Err(invalid_directory(
                path.as_os_str(),
                "the directory permissions are not 0700",
            ));
        }
        Ok(())
    }

    fn validate_sticky_temporary_directory(path: &Path) -> io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid_directory(
                path.as_os_str(),
                "the temporary root is not a physical directory",
            ));
        }
        if metadata.mode() & STICKY_BIT == 0 {
            return Err(invalid_directory(
                path.as_os_str(),
                "the temporary root does not have the sticky bit",
            ));
        }
        Ok(())
    }

    fn invalid_directory(path: &OsStr, reason: &'static str) -> io::Error {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{reason}: {}", Path::new(path).display()),
        )
    }

    #[cfg(test)]
    mod tests {
        use std::{
            fs,
            os::unix::fs::{DirBuilderExt as _, PermissionsExt as _},
            time::{SystemTime, UNIX_EPOCH},
        };

        use super::*;

        #[test]
        fn private_directory_validation_rejects_group_access() {
            let effective_uid = rustix::process::geteuid().as_raw();
            let root = std::env::temp_dir().join(format!(
                "nara-store-root-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            DirBuilder::new().mode(0o700).create(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o750)).unwrap();

            let error = validate_private_directory(&root, effective_uid).unwrap_err();

            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            fs::remove_dir(&root).unwrap();
        }
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
mod platform {
    use super::*;

    pub(super) fn generation_store_base_root() -> io::Result<PathBuf> {
        let root = std::env::temp_dir().join(STORE_ROOT_NAME);
        fs::create_dir_all(&root)?;
        let metadata = fs::symlink_metadata(&root)?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the supervised Play store root is not a physical directory",
            ));
        }
        Ok(root)
    }
}
