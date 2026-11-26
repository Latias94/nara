use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use nara_core::vfs::{AssetPath, DirAssetSource, write_atomic};
use nara_project::ProjectManifest;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{EditorDocument, EditorError};

const SETTINGS: &str = ".nara.ron";
const SETTINGS_LIMIT: u64 = 64 * 1024;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("invalid nara.toml: {0}")]
    Manifest(String),
    #[error("invalid .nara.ron: {0}")]
    Settings(#[from] ron::error::SpannedError),
    #[error("could not serialize .nara.ron: {0}")]
    Serialize(#[from] ron::Error),
    #[error("{0} does not name an authored scene or prefab")]
    NotDocument(AssetPath),
    #[error(transparent)]
    Document(#[from] EditorError),
}

/// An opened project's tooling manifest, asset root, and saved editor entry scene.
pub struct EditorProject {
    directory: PathBuf,
    assets: PathBuf,
    manifest: ProjectManifest,
    entry_scene: Option<AssetPath>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    scene: Option<String>,
}

impl EditorProject {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let directory = fs::canonicalize(directory)?;
        let manifest = ProjectManifest::parse_toml(&read_small(&directory.join("nara.toml"))?)
            .map_err(|error| ProjectError::Manifest(error.to_string()))?;
        let assets = directory.join(&manifest.paths.assets);
        let entry_scene = match read_small(&directory.join(SETTINGS)) {
            Ok(source) => ron::from_str::<Settings>(&source)?
                .scene
                .map(AssetPath::new)
                .transpose()?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(path) = &entry_scene
            && AssetKind::of(path) != AssetKind::Scene
        {
            return Err(ProjectError::NotDocument(path.clone()));
        }
        Ok(Self {
            directory,
            assets,
            manifest,
            entry_scene,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    pub fn entry_scene(&self) -> Option<&AssetPath> {
        self.entry_scene.as_ref()
    }

    pub fn asset_source(&self) -> io::Result<DirAssetSource> {
        let source = DirAssetSource::new(&self.assets)?;
        within(source.root(), &self.directory)?;
        Ok(source)
    }

    pub fn assets(&self) -> io::Result<AssetBrowserModel> {
        AssetBrowserModel::scan(self.asset_source()?.root())
    }

    /// Opens a scene or prefab on the project's asset mount.
    pub fn open_document(&self, path: &AssetPath) -> Result<EditorDocument, ProjectError> {
        if !matches!(AssetKind::of(path), AssetKind::Scene | AssetKind::Prefab) {
            return Err(ProjectError::NotDocument(path.clone()));
        }
        let root = self.asset_source()?;
        let physical = fs::canonicalize(root.root().join(path.as_str()))?;
        within(&physical, root.root())?;
        Ok(EditorDocument::open(physical)?)
    }

    pub fn set_entry_scene(&mut self, scene: Option<AssetPath>) -> Result<(), ProjectError> {
        if let Some(path) = &scene {
            if AssetKind::of(path) != AssetKind::Scene {
                return Err(ProjectError::NotDocument(path.clone()));
            }
            self.open_document(path)?;
        }
        let settings = Settings {
            scene: scene.as_ref().map(ToString::to_string),
        };
        let mut text = ron::ser::to_string_pretty(&settings, ron::ser::PrettyConfig::default())?;
        text.push('\n');
        write_atomic(&self.directory.join(SETTINGS), text.as_bytes())?;
        self.entry_scene = scene;
        Ok(())
    }
}

fn read_small(path: &Path) -> io::Result<String> {
    if !path.metadata()?.is_file() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut text = String::new();
    File::open(path)?
        .take(SETTINGS_LIMIT + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > SETTINGS_LIMIT {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(text)
}

fn within(candidate: &Path, root: &Path) -> io::Result<()> {
    if !candidate.starts_with(root) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Scene,
    Prefab,
    Image,
    File,
}

impl AssetKind {
    pub fn of(path: &AssetPath) -> Self {
        let name = path.as_str();
        if name.ends_with(".nara.scene.ron") {
            Self::Scene
        } else if name.ends_with(".nara.prefab.ron") {
            Self::Prefab
        } else if name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("png"))
        {
            Self::Image
        } else {
            Self::File
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub path: AssetPath,
    pub kind: AssetKind,
}

#[derive(Debug, Clone, Default)]
pub struct AssetBrowserModel {
    entries: Vec<AssetEntry>,
}

impl AssetBrowserModel {
    pub fn entries(&self) -> &[AssetEntry] {
        &self.entries
    }

    fn scan(root: &Path) -> io::Result<Self> {
        let mut pending = vec![(root.to_path_buf(), String::new())];
        let mut entries = Vec::new();
        while let Some((directory, relative)) = pending.pop() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let name = if relative.is_empty() {
                    name.to_owned()
                } else {
                    format!("{relative}/{name}")
                };
                let Ok(path) = AssetPath::new(name) else {
                    continue;
                };
                let kind = entry.file_type()?;
                // Skip symlinks and junctions so inspection cannot recurse through an alias.
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt as _;
                    if entry.metadata()?.file_attributes() & 0x400 != 0 {
                        continue;
                    }
                }
                if kind.is_dir() {
                    pending.push((entry.path(), path.to_string()));
                } else if kind.is_file() {
                    entries.push(AssetEntry {
                        kind: AssetKind::of(&path),
                        path,
                    });
                }
            }
        }
        entries.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        Ok(Self { entries })
    }
}
