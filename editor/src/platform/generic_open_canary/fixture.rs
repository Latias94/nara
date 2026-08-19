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
    AUTHORING_BUNDLE_PATH, EXTERNAL_PREFAB_PATH, EXTERNAL_SCENE_PATH, PROJECT_MANIFEST_PATH,
    RETAINED_SCRIPTS_ROOT_PATH, SCRIPTS_ROOT_PATH, SOURCE_MEMBER_BYTE_LIMIT, STARTUP_SCENE_PATH,
    TEMP_ROOT_PREFIX,
};

#[derive(Debug, Error)]
pub(super) enum GuardedProjectError {
    #[error("the source project root could not be inspected")]
    SourceRootInspect(#[source] io::Error),
    #[error("the source project root must be one real directory: {0}")]
    SourceRootInvalid(PathBuf),
    #[error("the source project root could not be opened")]
    SourceRootOpen(#[source] io::Error),
    #[error("the source project root did not satisfy untrusted capability policy")]
    SourceRootCapability(#[source] FsError),
    #[error("the fixed canary source member path is invalid: {path}")]
    SourceMemberPath {
        path: &'static str,
        #[source]
        source: PathValidationError,
    },
    #[error("the canary source must provide one safe regular member: {path}")]
    SourceMemberOpen {
        path: &'static str,
        #[source]
        source: FsError,
    },
    #[error("the canary source member could not be read within its byte limit: {path}")]
    SourceMemberRead {
        path: &'static str,
        #[source]
        source: FsError,
    },
    #[error("the guarded destination could not be prepared")]
    Destination(#[from] io::Error),
}

pub(super) struct GuardedProject {
    root: PathBuf,
    temp_parent: PathBuf,
    authoring_bundle: Vec<u8>,
}

impl GuardedProject {
    pub(super) fn prepare(source_root: &Path) -> Result<Self, GuardedProjectError> {
        let source_metadata =
            fs::symlink_metadata(source_root).map_err(GuardedProjectError::SourceRootInspect)?;
        if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
            return Err(GuardedProjectError::SourceRootInvalid(
                source_root.to_path_buf(),
            ));
        }
        let source_root = open_source_root(source_root)?;
        let authoring_bundle = read_source_member(&source_root, AUTHORING_BUNDLE_PATH)?;
        let temp_parent = env::temp_dir();
        let root = create_temp_root(&temp_parent)?;
        if let Err(error) = copy_project_without_bundle(&source_root, &root) {
            let _ = fs::remove_dir_all(&root);
            return Err(error);
        }
        Ok(Self {
            root,
            temp_parent,
            authoring_bundle,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn restore_authoring_bundle(&self) -> io::Result<()> {
        let destination = self.root.join(AUTHORING_BUNDLE_PATH);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&destination)?;
        let result = file
            .write_all(&self.authoring_bundle)
            .and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = result {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn replace_scripts_root(&self) -> io::Result<()> {
        let current = self.root.join(SCRIPTS_ROOT_PATH);
        let retained = self.root.join(RETAINED_SCRIPTS_ROOT_PATH);
        fs::rename(current, retained)?;
        fs::create_dir(self.root.join(SCRIPTS_ROOT_PATH))?;
        fs::write(
            self.root.join(SCRIPTS_ROOT_PATH).join("replacement.rs"),
            "fn replacement() {}\n",
        )
    }
}

impl Drop for GuardedProject {
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

pub(super) fn canary_prepare_error(error: GuardedProjectError) -> EditorRunError {
    EditorRunError::Platform(format!(
        "the generic Open/Recovery canary could not prepare its guarded project: {error}"
    ))
}

pub(super) fn canary_io_error(error: io::Error) -> EditorRunError {
    EditorRunError::Platform(format!(
        "the generic Open/Recovery canary could not repair its guarded project: {error}"
    ))
}

pub(super) fn canary_refresh_mutation_error(error: io::Error) -> EditorRunError {
    EditorRunError::Platform(format!(
        "the generic Open/Recovery canary could not replace its guarded scripts root: {error}"
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
        "could not allocate a unique guarded canary directory",
    ))
}

fn copy_project_without_bundle(
    source_root: &DirectoryCapability,
    destination_root: &Path,
) -> Result<(), GuardedProjectError> {
    for directory in [
        "blueprints",
        "catalogs",
        "media",
        "scripts",
        "src",
        "worlds",
    ] {
        fs::create_dir(destination_root.join(directory))?;
    }
    for relative in [
        "Cargo.toml",
        PROJECT_MANIFEST_PATH,
        EXTERNAL_PREFAB_PATH,
        "media/icon.png",
        "media/icon.png.meta",
        "scripts/tool.rs",
        "src/lib.rs",
        STARTUP_SCENE_PATH,
        EXTERNAL_SCENE_PATH,
        "worlds/notes.txt",
    ] {
        let bytes = read_source_member(source_root, relative)?;
        fs::write(destination_root.join(relative), bytes)?;
    }
    Ok(())
}

fn open_source_root(path: &Path) -> Result<DirectoryCapability, GuardedProjectError> {
    let file = open_directory_handle(path).map_err(GuardedProjectError::SourceRootOpen)?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
    )
    .map_err(GuardedProjectError::SourceRootCapability)
}

fn read_source_member(
    root: &DirectoryCapability,
    path: &'static str,
) -> Result<Vec<u8>, GuardedProjectError> {
    let relative = RelativePath::new(path)
        .map_err(|source| GuardedProjectError::SourceMemberPath { path, source })?;
    let file: FileCapability = root
        .open_file(&relative)
        .map_err(|source| GuardedProjectError::SourceMemberOpen { path, source })?;
    file.read_to_end_bounded(SOURCE_MEMBER_BYTE_LIMIT)
        .map_err(|source| GuardedProjectError::SourceMemberRead { path, source })
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
