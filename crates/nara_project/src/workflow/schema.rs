use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    process::ExitStatus,
};

use nara_core::vfs::write_atomic;
use nara_reflect::{ComponentCatalogFileError, ComponentCatalogFileLimits, ComponentSchemaCatalog};
use thiserror::Error;

use crate::play::{BuildError, BuildEvent, BuildRequest, build_interruptible, capture_stdout};

pub const SCHEMA_BINARY: &str = "nara_schema";
pub const SCHEMA_CACHE_PATH: &str = ".nara/schema_cache.json";

/// The caller's trust decision for executing the selected project's Cargo build and exporter.
/// Trust covers the workspace, its Cargo configuration and its build-time dependencies.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTrust {
    #[default]
    Untrusted,
    Trusted,
}

#[derive(Debug, Error)]
pub enum SchemaCacheError {
    #[error("building and running a Schema exporter requires a trusted project")]
    UntrustedProject,
    #[error("Schema refresh requires the {SCHEMA_BINARY} Cargo binary")]
    WrongBinary,
    #[error("the Schema exporter manifest must be in the selected project directory")]
    ManifestOutsideProject,
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error("could not run the Schema exporter: {0}")]
    ProducerIo(#[source] io::Error),
    #[error("the Schema exporter failed ({0})")]
    ProducerFailed(ExitStatus),
    #[error("Schema refresh was interrupted")]
    Interrupted,
    #[error("the Schema catalog exceeds {limit} bytes")]
    TooLarge { limit: usize },
    #[error("invalid Schema catalog: {0}")]
    InvalidCatalog(#[from] ComponentCatalogFileError),
    #[error("could not access the Schema cache: {0}")]
    CacheIo(#[from] io::Error),
}

/// Builds and runs the project's `nara_schema` target and publishes its stdout catalog.
///
/// Cargo diagnostics and the exporter's stderr are delivered to `emit`. The exporter must
/// write one catalog JSON document to stdout and exit successfully. Failed builds, unsuccessful
/// exits, and invalid catalogs leave the previous cache available through [`read_schema_cache`].
pub fn refresh_schema(
    request: &BuildRequest<'_>,
    trust: ProjectTrust,
    emit: impl FnMut(BuildEvent),
) -> Result<ComponentSchemaCatalog, SchemaCacheError> {
    refresh_schema_interruptible(request, trust, || false, emit)
}

/// Refreshes Schema and stops the active build or exporter when cancellation is requested.
/// The previous cache stays available if cancellation is observed before publication.
/// The output callback runs on the supervising thread and must return promptly.
pub fn refresh_schema_interruptible(
    request: &BuildRequest<'_>,
    trust: ProjectTrust,
    mut interrupted: impl FnMut() -> bool,
    mut emit: impl FnMut(BuildEvent),
) -> Result<ComponentSchemaCatalog, SchemaCacheError> {
    if trust != ProjectTrust::Trusted {
        return Err(SchemaCacheError::UntrustedProject);
    }
    if request.binary != SCHEMA_BINARY {
        return Err(SchemaCacheError::WrongBinary);
    }
    let project_dir = request.project_dir.canonicalize().map_err(BuildError::Io)?;
    let manifest_path = project_dir
        .join(request.manifest_path)
        .canonicalize()
        .map_err(BuildError::Io)?;
    if !manifest_path.starts_with(&project_dir) {
        return Err(SchemaCacheError::ManifestOutsideProject);
    }
    let request = BuildRequest {
        project_dir: &project_dir,
        manifest_path: &manifest_path,
        target: Some("host-tuple"),
        ..*request
    };

    let executable = match build_interruptible(&request, &mut interrupted, &mut emit) {
        Err(BuildError::Interrupted) => return Err(SchemaCacheError::Interrupted),
        result => result?,
    };
    if interrupted() {
        return Err(SchemaCacheError::Interrupted);
    }
    let limit = catalog_limit();
    let mut cancelled = false;
    let output = capture_stdout(
        &project_dir,
        &executable,
        &[],
        limit,
        || {
            cancelled = cancelled || interrupted();
            cancelled
        },
        |line| emit(BuildEvent::Output(line)),
    );
    let (status, stdout) = output.map_err(|error| {
        if error.kind() == io::ErrorKind::FileTooLarge {
            SchemaCacheError::TooLarge { limit }
        } else {
            SchemaCacheError::ProducerIo(error)
        }
    })?;
    finish_refresh(&project_dir, status, &stdout, || cancelled || interrupted())
}

/// Loads the last successful cache without building or executing the project.
pub fn read_schema_cache(
    project_dir: &Path,
) -> Result<Option<ComponentSchemaCatalog>, SchemaCacheError> {
    let file = match open_cache(project_dir) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let limit = catalog_limit();
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(SchemaCacheError::TooLarge { limit });
    }
    Ok(Some(ComponentSchemaCatalog::from_json_bytes(&bytes)?))
}

fn catalog_limit() -> usize {
    ComponentCatalogFileLimits::default().encoded_bytes().get()
}

pub(super) fn finish_refresh(
    project_dir: &Path,
    status: ExitStatus,
    stdout: &[u8],
    mut interrupted: impl FnMut() -> bool,
) -> Result<ComponentSchemaCatalog, SchemaCacheError> {
    if interrupted() {
        return Err(SchemaCacheError::Interrupted);
    }
    if !status.success() {
        return Err(SchemaCacheError::ProducerFailed(status));
    }
    let catalog = ComponentSchemaCatalog::from_json_bytes(stdout)?;
    if interrupted() {
        return Err(SchemaCacheError::Interrupted);
    }
    write_atomic(&cache_path(project_dir, true)?, stdout)?;
    Ok(catalog)
}

fn open_cache(project_dir: &Path) -> io::Result<File> {
    let path = cache_path(project_dir, false)?;
    if !fs::symlink_metadata(&path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the Schema cache must be a regular file",
        ));
    }
    File::open(path)
}

fn cache_path(project_dir: &Path, create: bool) -> io::Result<PathBuf> {
    let root = project_dir.canonicalize()?;
    let directory = root.join(".nara");
    if create {
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    if !fs::symlink_metadata(&directory)?.is_dir()
        || directory.canonicalize()?.parent() != Some(root.as_path())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the Schema cache directory must be a direct project directory",
        ));
    }
    Ok(root.join(SCHEMA_CACHE_PATH))
}
