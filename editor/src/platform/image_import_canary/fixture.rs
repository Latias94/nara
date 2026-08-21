use std::{
    env,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use nara::fs::{
    CapabilityRights, DirectoryCapability, FileCapability, FsError, HostCapabilityOptions,
    PathValidationError, RelativePath, TrustMode,
};
use thiserror::Error;

use super::super::EditorRunError;
use super::{
    ASSET_SOURCE_PATH, AUTHORING_BUNDLE_PATH, PREFAB_PATH, PROJECT_MANIFEST_PATH,
    SOURCE_MEMBER_BYTE_LIMIT, STARTUP_SCENE_PATH, TEMP_ROOT_PREFIX,
};

const FIXED_SOURCE_MEMBERS: &[&str] = &[
    "Cargo.toml",
    PROJECT_MANIFEST_PATH,
    ASSET_SOURCE_PATH,
    PREFAB_PATH,
    STARTUP_SCENE_PATH,
    AUTHORING_BUNDLE_PATH,
    "scripts/tool.rs",
    "src/lib.rs",
];

#[derive(Debug, Error)]
pub(super) enum GuardedImageProjectError {
    #[error("the image canary source project could not be inspected")]
    SourceRootInspect(#[source] io::Error),
    #[error("the image canary source project must be one real directory")]
    SourceRootInvalid,
    #[error("the image canary source project could not be opened")]
    SourceRootOpen(#[source] io::Error),
    #[error("the image canary source project did not satisfy untrusted capability policy")]
    SourceRootCapability(#[source] FsError),
    #[error("the fixed image canary source member path is invalid: {path}")]
    SourceMemberPath {
        path: &'static str,
        #[source]
        source: PathValidationError,
    },
    #[error("the image canary source must provide one safe regular member: {path}")]
    SourceMemberOpen {
        path: &'static str,
        #[source]
        source: FsError,
    },
    #[error("the image canary source member exceeded its byte limit: {path}")]
    SourceMemberRead {
        path: &'static str,
        #[source]
        source: FsError,
    },
    #[error("the guarded image canary project could not be prepared")]
    Destination(#[from] io::Error),
}

pub(super) struct GuardedImageProject {
    root: PathBuf,
    temp_parent: PathBuf,
}

impl GuardedImageProject {
    pub(super) fn prepare(source_root: &Path) -> Result<Self, GuardedImageProjectError> {
        let source_metadata = fs::symlink_metadata(source_root)
            .map_err(GuardedImageProjectError::SourceRootInspect)?;
        if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
            return Err(GuardedImageProjectError::SourceRootInvalid);
        }
        let source_root = open_source_root(source_root)?;
        let temp_parent = env::temp_dir();
        let root = create_temp_root(&temp_parent)?;
        if let Err(error) = copy_project(&source_root, &root) {
            let _ = fs::remove_dir_all(&root);
            return Err(error);
        }
        Ok(Self { root, temp_parent })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn corrupt_import_source(&self) -> io::Result<()> {
        let mut source = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(self.root.join(ASSET_SOURCE_PATH))?;
        source.write_all(b"not a PNG")?;
        source.sync_all()
    }

    pub(super) fn project_rust_was_not_executed(&self) -> bool {
        !self.root.join("target").exists()
            && source_contains_compile_sentinel(&self.root.join("src/lib.rs"))
            && source_contains_compile_sentinel(&self.root.join("scripts/tool.rs"))
    }
}

impl Drop for GuardedImageProject {
    fn drop(&mut self) {
        let safe_name = self
            .root
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with(TEMP_ROOT_PREFIX));
        if self.root.parent() == Some(self.temp_parent.as_path()) && safe_name {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

pub(super) fn canary_prepare_error(error: GuardedImageProjectError) -> EditorRunError {
    EditorRunError::Platform(format!(
        "the image import canary could not prepare its guarded project: {error}"
    ))
}

pub(super) fn canary_corrupt_error(error: io::Error) -> EditorRunError {
    EditorRunError::Platform(format!(
        "the image import canary could not corrupt its guarded source: {error}"
    ))
}

fn create_temp_root(temp_parent: &Path) -> io::Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..32_u8 {
        let candidate = temp_parent.join(format!(
            "{TEMP_ROOT_PREFIX}{}-{stamp}-{attempt}",
            process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique guarded image canary directory",
    ))
}

fn copy_project(
    source_root: &DirectoryCapability,
    destination_root: &Path,
) -> Result<(), GuardedImageProjectError> {
    for directory in [
        "assets/textures",
        "prefabs",
        "scenes",
        "schema",
        "scripts",
        "src",
    ] {
        fs::create_dir_all(destination_root.join(directory))?;
    }
    for &relative in FIXED_SOURCE_MEMBERS {
        let bytes = read_source_member(source_root, relative)?;
        fs::write(destination_root.join(relative), bytes)?;
    }
    Ok(())
}

fn open_source_root(path: &Path) -> Result<DirectoryCapability, GuardedImageProjectError> {
    let file = open_directory_handle(path).map_err(GuardedImageProjectError::SourceRootOpen)?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
    )
    .map_err(GuardedImageProjectError::SourceRootCapability)
}

fn read_source_member(
    root: &DirectoryCapability,
    path: &'static str,
) -> Result<Vec<u8>, GuardedImageProjectError> {
    let relative = RelativePath::new(path)
        .map_err(|source| GuardedImageProjectError::SourceMemberPath { path, source })?;
    let file: FileCapability = root
        .open_file(&relative)
        .map_err(|source| GuardedImageProjectError::SourceMemberOpen { path, source })?;
    file.read_to_end_bounded(SOURCE_MEMBER_BYTE_LIMIT)
        .map_err(|source| GuardedImageProjectError::SourceMemberRead { path, source })
}

fn source_contains_compile_sentinel(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|source| source.contains("compile_error!"))
}

#[cfg(windows)]
fn open_directory_handle(path: &Path) -> io::Result<File> {
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
fn open_directory_handle(path: &Path) -> io::Result<File> {
    File::open(path)
}
