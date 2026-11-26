use std::{
    fs::File,
    io::{self, Read as _},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

use super::GAME_BINARY;
use crate::{
    ProjectManifest,
    manifest::{nonempty, optional_nonempty},
};

const MANIFEST_BYTE_LIMIT: u64 = 256 * 1024;

#[derive(Debug, Error)]
pub enum ProjectCheckError {
    #[error("could not read {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid {}: {source}", .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

/// Reads `nara.toml` and the local Cargo package declaration without running Cargo.
pub fn check_project(project_dir: &Path) -> Result<ProjectManifest, ProjectCheckError> {
    let manifest = read_manifest(project_dir.join("nara.toml"))?;
    CargoPackage::read(project_dir)?;
    Ok(manifest)
}

/// Cargo manifest fields used to select a local binary and its required features.
#[derive(Debug, Deserialize)]
pub struct CargoPackage {
    package: Package,
    #[serde(default, rename = "bin")]
    targets: Vec<Binary>,
}

impl CargoPackage {
    pub fn read(project_dir: &Path) -> Result<Self, ProjectCheckError> {
        read_manifest(project_dir.join("Cargo.toml"))
    }

    pub fn name(&self) -> &str {
        &self.package.name
    }

    pub fn default_binary(&self) -> &str {
        self.package.default_run.as_deref().unwrap_or(GAME_BINARY)
    }

    pub fn required_features(&self, binary: &str) -> &[String] {
        self.targets
            .iter()
            .find(|target| target.name == binary)
            .map_or(&[], |target| &target.required_features)
    }
}

#[derive(Debug, Deserialize)]
struct Package {
    #[serde(deserialize_with = "nonempty")]
    name: String,
    #[serde(
        default,
        rename = "default-run",
        deserialize_with = "optional_nonempty"
    )]
    default_run: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Binary {
    #[serde(deserialize_with = "nonempty")]
    name: String,
    #[serde(default, rename = "required-features")]
    required_features: Vec<String>,
}

fn read_manifest<T: serde::de::DeserializeOwned>(path: PathBuf) -> Result<T, ProjectCheckError> {
    let read = || {
        if !path.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected a file",
            ));
        }
        let mut contents = String::new();
        File::open(&path)?
            .take(MANIFEST_BYTE_LIMIT + 1)
            .read_to_string(&mut contents)?;
        if contents.len() as u64 > MANIFEST_BYTE_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                format!("manifest exceeds {MANIFEST_BYTE_LIMIT} bytes"),
            ));
        }
        Ok(contents)
    };
    let contents = read().map_err(|source| ProjectCheckError::Read {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&contents).map_err(|source| ProjectCheckError::Parse { path, source })
}
