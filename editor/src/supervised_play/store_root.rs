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
        os::unix::fs::{DirBuilderExt as _, MetadataExt as _},
        path::{Path, PathBuf},
    };

    use super::STORE_ROOT_NAME;

    const PRIVATE_DIRECTORY_MODE: u32 = 0o700;

    pub(super) fn generation_store_base_root() -> io::Result<PathBuf> {
        let effective_uid = rustix::process::geteuid().as_raw();
        let cache = cache_base_directory(
            env::var_os("XDG_CACHE_HOME").as_deref(),
            env::var_os("HOME").as_deref(),
        )?;
        create_cache_store_root(&cache, effective_uid)
    }

    fn cache_base_directory(cache: Option<&OsStr>, home: Option<&OsStr>) -> io::Result<PathBuf> {
        if let Some(cache) = cache.filter(|value| !value.is_empty()) {
            let cache = Path::new(cache);
            if cache.is_absolute() {
                return Ok(cache.to_path_buf());
            }
        }
        // XDG ignores relative base-directory values. A missing or invalid HOME cannot
        // authorize a relative fallback, and runtime directories are unsuitable for large images.
        let home = home
            .map(Path::new)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the user cache requires an absolute XDG_CACHE_HOME or HOME",
                )
            })?;
        Ok(home.join(".cache"))
    }

    fn create_cache_store_root(cache: &Path, effective_uid: u32) -> io::Result<PathBuf> {
        // This is a cooperative local Host path, not a no-follow proof for every ancestor.
        // Existing cache permissions remain unchanged; only the engine-owned leaf is private.
        DirBuilder::new()
            .recursive(true)
            .mode(PRIVATE_DIRECTORY_MODE)
            .create(cache)?;
        validate_cache_directory(cache, effective_uid)?;
        let root = cache.join(STORE_ROOT_NAME);
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

    fn validate_cache_directory(path: &Path, effective_uid: u32) -> io::Result<()> {
        let metadata = validate_owned_directory(path, effective_uid)?;
        if metadata.mode() & 0o022 != 0 {
            return Err(invalid_directory(
                path.as_os_str(),
                "the cache directory is writable by another user",
            ));
        }
        Ok(())
    }

    fn validate_private_directory(path: &Path, effective_uid: u32) -> io::Result<()> {
        let metadata = validate_owned_directory(path, effective_uid)?;
        if metadata.mode() & 0o777 != PRIVATE_DIRECTORY_MODE {
            return Err(invalid_directory(
                path.as_os_str(),
                "the directory permissions are not 0700",
            ));
        }
        Ok(())
    }

    fn validate_owned_directory(path: &Path, effective_uid: u32) -> io::Result<fs::Metadata> {
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
        Ok(metadata)
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
            os::unix::fs::{DirBuilderExt as _, PermissionsExt as _, symlink},
            time::{SystemTime, UNIX_EPOCH},
        };

        use super::*;

        #[test]
        fn cache_selection_obeys_absolute_xdg_and_home_fallback() {
            let home = Some(OsStr::new("/home/user"));
            assert_eq!(
                cache_base_directory(Some(OsStr::new("/cache/custom")), None).unwrap(),
                Path::new("/cache/custom")
            );
            for cache in [
                None,
                Some(OsStr::new("")),
                Some(OsStr::new("relative/cache")),
            ] {
                assert_eq!(
                    cache_base_directory(cache, home).unwrap(),
                    Path::new("/home/user/.cache")
                );
                for invalid_home in [
                    None,
                    Some(OsStr::new("")),
                    Some(OsStr::new("relative/home")),
                ] {
                    assert_eq!(
                        cache_base_directory(cache, invalid_home)
                            .unwrap_err()
                            .kind(),
                        io::ErrorKind::InvalidInput
                    );
                }
            }
        }

        #[test]
        fn missing_cache_is_created_with_a_private_engine_store() {
            let temporary = TestDirectory::new();
            let cache = temporary.path.join("cache/nested");
            let root = create_cache_store_root(&cache, effective_uid()).unwrap();
            assert_eq!(root, cache.join(STORE_ROOT_NAME));
            assert_eq!(mode(&cache), 0o700);
            assert_eq!(mode(&root), 0o700);
            assert_eq!(fs::symlink_metadata(root).unwrap().uid(), effective_uid());
        }

        #[test]
        fn existing_user_cache_keeps_0755_permissions_and_private_store_identity() {
            let temporary = TestDirectory::new();
            fs::set_permissions(&temporary.path, fs::Permissions::from_mode(0o755)).unwrap();
            let root = create_cache_store_root(&temporary.path, effective_uid()).unwrap();
            let identity = fs::metadata(&root).unwrap().ino();
            assert_eq!(mode(&temporary.path), 0o755);
            assert_eq!(mode(&root), 0o700);
            assert_eq!(
                create_cache_store_root(&temporary.path, effective_uid()).unwrap(),
                root
            );
            assert_eq!(fs::metadata(&root).unwrap().ino(), identity);
        }

        #[test]
        fn writable_cache_and_nonprivate_store_are_rejected_without_chmod() {
            let temporary = TestDirectory::new();
            for permissions in [0o770, 0o707] {
                fs::set_permissions(&temporary.path, fs::Permissions::from_mode(permissions))
                    .unwrap();
                assert_eq!(
                    create_cache_store_root(&temporary.path, effective_uid())
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::PermissionDenied
                );
                assert_eq!(mode(&temporary.path), permissions);
                assert!(!temporary.path.join(STORE_ROOT_NAME).exists());
            }
            fs::set_permissions(&temporary.path, fs::Permissions::from_mode(0o755)).unwrap();
            let root = temporary.path.join(STORE_ROOT_NAME);
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o750)).unwrap();
            assert_eq!(
                create_cache_store_root(&temporary.path, effective_uid())
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            assert_eq!(mode(&root), 0o750);
            assert_eq!(mode(&temporary.path), 0o755);
        }

        #[test]
        fn cache_and_store_symlinks_are_rejected_before_use() {
            let temporary = TestDirectory::new();
            let physical = temporary.path.join("physical");
            DirBuilder::new().mode(0o700).create(&physical).unwrap();
            let cache_link = temporary.path.join("cache-link");
            symlink(&physical, &cache_link).unwrap();
            assert_eq!(
                create_cache_store_root(&cache_link, effective_uid())
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            assert!(!physical.join(STORE_ROOT_NAME).exists());
            let leaf = temporary.path.join(STORE_ROOT_NAME);
            symlink(&physical, &leaf).unwrap();
            assert_eq!(
                create_cache_store_root(&temporary.path, effective_uid())
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            assert!(
                fs::symlink_metadata(&leaf)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read_dir(physical).unwrap().count(), 0);
        }

        #[test]
        fn directory_validation_rejects_a_different_owner_identity() {
            let temporary = TestDirectory::new();
            let other_uid = effective_uid().wrapping_add(1);
            assert_eq!(
                validate_cache_directory(&temporary.path, other_uid)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            assert_eq!(
                validate_private_directory(&temporary.path, other_uid)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            assert_eq!(mode(&temporary.path), 0o700);
        }

        fn effective_uid() -> u32 {
            rustix::process::geteuid().as_raw()
        }

        fn mode(path: &Path) -> u32 {
            fs::symlink_metadata(path).unwrap().mode() & 0o777
        }

        struct TestDirectory {
            path: PathBuf,
        }

        impl TestDirectory {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "nara-store-root-test-{}-{}",
                    std::process::id(),
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ));
                DirBuilder::new().mode(0o700).create(&path).unwrap();
                Self { path }
            }
        }

        impl Drop for TestDirectory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.path);
            }
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
