//! Incremental, source-independent game delivery for the current host.

mod assets;
mod publish;
#[cfg(test)]
mod tests;

use std::{
    fs,
    io::{self, Write as _},
    path::{self, Path, PathBuf},
};

use nara_asset::{AssetScanError, AssetScanMode, cooked::NpakError};
use nara_core::AssetId;
use nara_reflect::{ComponentCodecError, ComponentRegistryError, ComponentSchemaCatalog};
use nara_scene::authoring::prefab::PrefabError;
use thiserror::Error;

use self::assets::Cooker;
use super::{
    CargoPackage, ProjectCheckError, SCHEMA_BINARY, SchemaCacheError, read_project_manifest,
    read_schema_cache,
    schema::{read_latest_schema_cache, refresh_at},
};
use crate::{
    ProjectDelivery,
    play::{BuildError, BuildEvent, BuildRequest, ProjectTrust, build_interruptible},
};

/// File placed beside the standalone executable.
pub const GAME_ARCHIVE: &str = "game.npak";
const PROJECT_STATE: &str = ".nara";

#[derive(Debug)]
pub struct DeliveryReport {
    pub archive: PathBuf,
    pub executable: Option<PathBuf>,
    pub bytes: u64,
    pub assets: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

#[derive(Debug, Error)]
pub enum DeliveryError {
    #[error("declare the stable entry scene IDs in [delivery] in nara.toml")]
    MissingDeliveryProfile,
    #[error(
        "the {id} asset at {path} has no matching Schema for '{component}' version {version}; refresh the Schema cache and verify that the component is registered"
    )]
    MissingComponentSchema {
        id: AssetId,
        path: String,
        component: String,
        version: u32,
    },
    #[error("building a standalone game requires a trusted project")]
    UntrustedProject,
    #[error("delivery was interrupted")]
    Interrupted,
    #[error("the export directory already exists: {}", .0.display())]
    DestinationExists(PathBuf),
    #[error("the exported game must have a filename distinct from {GAME_ARCHIVE}")]
    InvalidExecutableName,
    #[error(transparent)]
    Project(#[from] ProjectCheckError),
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error(transparent)]
    SchemaCache(#[from] SchemaCacheError),
    #[error(transparent)]
    Schema(#[from] ComponentCodecError),
    #[error(transparent)]
    Catalog(#[from] ComponentRegistryError),
    #[error(transparent)]
    AssetScan(#[from] AssetScanError),
    #[error("asset {0} is not in the project asset database")]
    MissingAsset(AssetId),
    #[error("expected {expected} for asset {id} at {path}, found {importer}")]
    AssetImporter {
        id: AssetId,
        path: String,
        expected: &'static str,
        importer: String,
    },
    #[error("could not cook asset {id} at {path}: {message}")]
    Asset {
        id: AssetId,
        path: String,
        message: String,
    },
    #[error(transparent)]
    Prefab(#[from] PrefabError),
    #[error(transparent)]
    Archive(#[from] NpakError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl DeliveryError {
    #[must_use]
    pub fn is_interrupted(&self) -> bool {
        matches!(
            self,
            Self::Interrupted
                | Self::Build(BuildError::Interrupted)
                | Self::SchemaCache(
                    SchemaCacheError::Interrupted
                        | SchemaCacheError::Build(BuildError::Interrupted)
                )
        )
    }
}

/// Bakes entry scenes and their dependency closure into `.nara/game.npak`.
/// Untrusted Cook reads existing sidecars and Schema caches and only writes project state.
pub fn cook_project(
    project: &Path,
    trust: ProjectTrust,
    mut stop: impl FnMut() -> bool,
    mut emit: impl FnMut(BuildEvent),
) -> Result<DeliveryReport, DeliveryError> {
    let project = project.canonicalize()?;
    let (profile, mut cooker) = prepare(&project, trust, &mut stop, &mut emit)?;
    let pack = cooker.cook(&profile.scenes, &mut stop)?;
    let state = project.join(PROJECT_STATE);
    let archive = state.join(GAME_ARCHIVE);
    let temporary = tempfile::NamedTempFile::new_in(state)?;
    let bytes = write_archive(&pack, temporary.as_file())?;
    check_stop(&mut stop)?;
    temporary.persist(&archive).map_err(|error| error.error)?;
    Ok(cooker.report(archive, None, bytes, pack.len()))
}

/// Builds the current host's release executable and publishes it beside a cooked archive.
/// The destination's parent must exist. An existing destination is left intact.
pub fn export_project(
    project: &Path,
    output: &Path,
    trust: ProjectTrust,
    mut stop: impl FnMut() -> bool,
    mut emit: impl FnMut(BuildEvent),
) -> Result<DeliveryReport, DeliveryError> {
    if trust != ProjectTrust::Trusted {
        return Err(DeliveryError::UntrustedProject);
    }
    let output = path::absolute(output)?;
    let invalid_destination = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected an export directory name",
        )
    };
    let name = output.file_name().ok_or_else(invalid_destination)?;
    let parent = output.parent().ok_or_else(invalid_destination)?;
    ensure_new_destination(&output)?;
    let parent = parent.canonicalize()?;
    let output = parent.join(name);
    let project = project.canonicalize()?;
    let (profile, mut cooker) = prepare(&project, trust, &mut stop, &mut emit)?;
    let pack = cooker.cook(&profile.scenes, &mut stop)?;
    let package = CargoPackage::read(&project)?;
    let binary = package.default_binary();
    let features = build_features(&package, binary, &profile.features);
    let artifacts = tempfile::tempdir_in(&parent)?;
    let request = BuildRequest {
        features: &features,
        release: true,
        no_default_features: true,
        artifact_directory: Some(artifacts.path()),
        ..BuildRequest::new(&project, package.name(), binary)
    };
    let executable = build_interruptible(&request, trust, &mut stop, &mut emit)?;
    let executable_name = executable
        .file_name()
        .filter(|name| {
            !name
                .as_encoded_bytes()
                .eq_ignore_ascii_case(GAME_ARCHIVE.as_bytes())
        })
        .ok_or(DeliveryError::InvalidExecutableName)?;
    let temporary = tempfile::tempdir_in(&parent)?;
    fs::copy(&executable, temporary.path().join(executable_name))?;
    let archive = fs::File::create_new(temporary.path().join(GAME_ARCHIVE))?;
    let bytes = write_archive(&pack, &archive)?;
    drop(archive);
    check_stop(&mut stop)?;
    publish::directory(temporary.path(), &output).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists || fs::symlink_metadata(&output).is_ok() {
            DeliveryError::DestinationExists(output.clone())
        } else {
            error.into()
        }
    })?;
    let _ = temporary.keep();
    Ok(cooker.report(
        output.join(GAME_ARCHIVE),
        Some(output.join(executable_name)),
        bytes,
        pack.len(),
    ))
}

fn prepare(
    project: &Path,
    trust: ProjectTrust,
    stop: &mut impl FnMut() -> bool,
    emit: &mut impl FnMut(BuildEvent),
) -> Result<(ProjectDelivery, Cooker), DeliveryError> {
    check_stop(stop)?;
    let manifest = read_project_manifest(project)?;
    let profile = manifest
        .delivery
        .ok_or(DeliveryError::MissingDeliveryProfile)?;
    let project_schemas = if profile.scenes.is_empty() {
        ComponentSchemaCatalog::default()
    } else {
        load_schemas(project, &profile, trust, stop, emit)?
    };
    let scan_mode = match trust {
        ProjectTrust::Trusted => AssetScanMode::Discover,
        ProjectTrust::Untrusted => AssetScanMode::ReadOnly,
    };
    let cooker = Cooker::open(project, &manifest.paths.assets, scan_mode, project_schemas)?;
    Ok((profile, cooker))
}

fn load_schemas(
    project: &Path,
    profile: &ProjectDelivery,
    trust: ProjectTrust,
    stop: &mut impl FnMut() -> bool,
    emit: &mut impl FnMut(BuildEvent),
) -> Result<ComponentSchemaCatalog, DeliveryError> {
    let trusted = trust == ProjectTrust::Trusted;
    if !trusted && !project.join("Cargo.toml").try_exists()? {
        return Ok(read_schema_cache(project)?.unwrap_or_default());
    }
    let package = CargoPackage::read(project)?;
    let game = package.default_binary();
    let mut features = build_features(&package, game, &profile.features);
    features.extend(
        package
            .required_features(SCHEMA_BINARY)
            .iter()
            .map(String::as_str),
    );
    features.sort_unstable();
    features.dedup();
    let cache = schema_cache_path(package.name(), game, &features);
    if trusted && package.has_binary(project, SCHEMA_BINARY) {
        let artifacts = tempfile::tempdir()?;
        let request = BuildRequest {
            features: &features,
            release: true,
            no_default_features: true,
            artifact_directory: Some(artifacts.path()),
            ..BuildRequest::new(project, package.name(), SCHEMA_BINARY)
        };
        return Ok(refresh_at(&request, trust, &cache, stop, emit)?);
    }
    Ok(read_latest_schema_cache(project, &cache)?.unwrap_or_default())
}

fn schema_cache_path(package: &str, game: &str, features: &[&str]) -> String {
    let mut digest = blake3::Hasher::new();
    digest.update(b"nara.delivery.schema.release.v1\0");
    let platform = assets::host_platform();
    for piece in [platform.as_str(), package, game]
        .into_iter()
        .chain(features.iter().copied())
    {
        digest.update(&(piece.len() as u64).to_le_bytes());
        digest.update(piece.as_bytes());
    }
    format!(
        "{PROJECT_STATE}/delivery-schema-{}.json",
        digest.finalize().to_hex()
    )
}

fn build_features<'a>(
    package: &'a CargoPackage,
    binary: &str,
    extra: &'a [String],
) -> Vec<&'a str> {
    let mut features: Vec<_> = package
        .required_features(binary)
        .iter()
        .chain(extra)
        .map(String::as_str)
        .collect();
    features.sort_unstable();
    features.dedup();
    features
}

fn write_archive(pack: &assets::Pack, mut file: &fs::File) -> Result<u64, DeliveryError> {
    pack.write(&mut file)?;
    file.flush()?;
    file.sync_all()?;
    Ok(file.metadata()?.len())
}

fn ensure_new_destination(output: &Path) -> Result<(), DeliveryError> {
    match fs::symlink_metadata(output) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err(DeliveryError::DestinationExists(output.to_owned())),
    }
}

fn check_stop(stop: &mut impl FnMut() -> bool) -> Result<(), DeliveryError> {
    if stop() {
        Err(DeliveryError::Interrupted)
    } else {
        Ok(())
    }
}
