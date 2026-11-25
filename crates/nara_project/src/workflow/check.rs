use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use nara_core::diagnostic::DiagnosticReport;
use nara_core::vfs::{AssetPath, DirAssetSource};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleLimits, ComponentSchema, ComponentTypeId,
    PersistentComponentSchemaOwnerId, collect_declared_asset_references_from_schema,
    validate_component_value,
};
use nara_scene::{
    PrefabDocumentCandidate, SceneDocumentCandidate, SceneEntityRecord, SceneFileLimits,
};
use thiserror::Error;

use super::{SchemaCacheError, read_schema_cache};
use crate::{DEFAULT_MANIFEST_BYTE_LIMIT, ProjectManifest, ProjectProfileError};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProjectCheck {
    pub scenes: usize,
    pub prefabs: usize,
    pub asset_paths: usize,
    /// Components or references that require a newer Schema or native asset resolution.
    pub deferred: usize,
}

#[derive(Debug, Error)]
pub enum ProjectCheckError {
    #[error("could not read {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid nara.toml")]
    Manifest { diagnostics: Box<DiagnosticReport> },
    #[error(transparent)]
    Profile(#[from] ProjectProfileError),
    #[error(transparent)]
    Cache(#[from] SchemaCacheError),
    #[error("invalid {path}: {source}")]
    Source {
        path: String,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

/// Validates the manifest, optional Schema files, declared Scene roots, and their direct
/// path-based asset references without invoking Cargo or loading native code.
/// Path-based Prefab sources are checked recursively.
/// Unavailable Schema, stable asset IDs, and Prefab overrides are left for runtime admission.
pub fn check_project(
    root: &Path,
    profile: Option<&str>,
) -> Result<ProjectCheck, ProjectCheckError> {
    let files = ProjectFiles {
        root,
        source: DirAssetSource::new(root).map_err(|source| ProjectCheckError::Read {
            path: root.to_path_buf(),
            source,
        })?,
    };
    let manifest = files.read("nara.toml", DEFAULT_MANIFEST_BYTE_LIMIT)?;
    let loaded = ProjectManifest::parse_toml_bytes(&manifest);
    if loaded.has_errors() {
        return Err(ProjectCheckError::Manifest {
            diagnostics: Box::new(loaded.diagnostics),
        });
    }
    let manifest = loaded.manifest.ok_or_else(|| ProjectCheckError::Manifest {
        diagnostics: Box::new(loaded.diagnostics),
    })?;
    let settings = manifest.resolve_profile(profile)?;
    let bundle = settings
        .authoring_schema_bundle
        .as_ref()
        .map(|path| {
            let path = path.as_str();
            let bytes = files.read(
                path,
                AuthoringSchemaBundleLimits::default().encoded_bytes().get() as u64,
            )?;
            AuthoringSchemaBundle::from_json_bytes(&bytes)
                .map_err(|source| invalid_source(path, source))
        })
        .transpose()?;
    let cache = read_schema_cache(root)?;
    let mut schemas = BTreeMap::new();
    if let Some(cache) = &cache {
        schemas.extend(
            cache
                .components()
                .iter()
                .map(|schema| (schema.id(), (schema, None))),
        );
    }
    if let Some(bundle) = &bundle {
        for owner in bundle.owners() {
            schemas.extend(
                owner
                    .current()
                    .components()
                    .iter()
                    .map(|schema| (schema.id(), (schema, Some(owner.owner_id())))),
            );
        }
    }
    let mut check = ContentCheck {
        files: &files,
        schemas,
        assets_dir: settings.paths.assets.as_str(),
        prefabs_dir: settings.paths.prefabs.as_str(),
        assets: BTreeSet::new(),
        prefabs: BTreeSet::new(),
        pending_prefabs: Vec::new(),
        report: ProjectCheck::default(),
    };
    let scenes: BTreeSet<_> = settings
        .startup
        .default_scene
        .iter()
        .chain(&settings.content.scenes)
        .map(|path| format!("{}/{path}", settings.paths.scenes))
        .collect();
    let scene_limit = SceneFileLimits::default().encoded_bytes().get() as u64;
    for path in &scenes {
        let bytes = files.read(path, scene_limit)?;
        let scene = if path.ends_with(".ron") {
            SceneDocumentCandidate::decode_ron_bytes(&bytes)
        } else {
            SceneDocumentCandidate::decode_json_bytes(&bytes)
        }
        .map_err(|source| invalid_source(path, source))?;
        let scene = scene
            .into_composition_source()
            .map_err(|source| invalid_source(path, source))?;
        check.entities(path, &scene.entities)?;
    }
    while let Some(path) = check.pending_prefabs.pop() {
        let bytes = files.read(&path, scene_limit)?;
        let prefab = if path.ends_with(".ron") {
            PrefabDocumentCandidate::decode_ron_bytes(&bytes)
        } else {
            PrefabDocumentCandidate::decode_json_bytes(&bytes)
        }
        .map_err(|source| invalid_source(&path, source))?;
        let prefab = prefab
            .into_composition_source()
            .map_err(|source| invalid_source(&path, source))?;
        check.entities(&path, &prefab.entities)?;
    }
    if let Some(path) = &settings.input.action_map {
        drop(files.open(path.as_str())?);
    }
    check.report.scenes = scenes.len();
    check.report.prefabs = check.prefabs.len();
    check.report.asset_paths = check.assets.len();
    Ok(check.report)
}

struct ContentCheck<'a> {
    files: &'a ProjectFiles<'a>,
    schemas: BTreeMap<
        &'a ComponentTypeId,
        (
            &'a ComponentSchema,
            Option<&'a PersistentComponentSchemaOwnerId>,
        ),
    >,
    assets_dir: &'a str,
    prefabs_dir: &'a str,
    assets: BTreeSet<String>,
    prefabs: BTreeSet<String>,
    pending_prefabs: Vec<String>,
    report: ProjectCheck,
}

impl ContentCheck<'_> {
    fn entities(
        &mut self,
        source: &str,
        entities: &[SceneEntityRecord],
    ) -> Result<(), ProjectCheckError> {
        for entity in entities {
            if let Some(prefab) = &entity.prefab {
                if let Some(path) = prefab.source.as_path() {
                    let path = format!("{}/{path}", self.prefabs_dir);
                    if self.prefabs.insert(path.clone()) {
                        self.pending_prefabs.push(path);
                    }
                } else {
                    self.report.deferred += 1;
                }
                self.report.deferred += usize::from(prefab.overrides.is_some());
            }
            for (component, record) in &entity.components {
                let Some((schema, _)) = self.schemas.get(component).filter(|(schema, owner)| {
                    schema.version() == record.version
                        && record.contextual_defaults.is_empty()
                        && owner
                            .zip(record.owner())
                            .is_none_or(|(expected, actual)| expected == actual)
                }) else {
                    self.report.deferred += 1;
                    continue;
                };
                let location = || format!("{source} entity {} component {component}", entity.id);
                validate_component_value(schema, record.value())
                    .map_err(|source| invalid_source(location(), source))?;
                let references = collect_declared_asset_references_from_schema(
                    schema,
                    record.version,
                    record.value(),
                )
                .map_err(|source| invalid_source(location(), source))?;
                for reference in references {
                    if let Some(path) = reference.asset_ref().as_path() {
                        let path = format!("{}/{path}", self.assets_dir);
                        if self.assets.insert(path.clone()) {
                            drop(self.files.open(&path)?);
                        }
                    } else {
                        self.report.deferred += 1;
                    }
                }
            }
        }
        Ok(())
    }
}

struct ProjectFiles<'a> {
    root: &'a Path,
    source: DirAssetSource,
}

impl ProjectFiles<'_> {
    fn open(&self, path: &str) -> Result<File, ProjectCheckError> {
        let open = || self.source.open(&AssetPath::new(path)?);
        open().map_err(|source| self.read_error(path, source))
    }

    fn read(&self, path: &str, limit: u64) -> Result<Vec<u8>, ProjectCheckError> {
        let mut bytes = Vec::new();
        self.open(path)?
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| self.read_error(path, source))?;
        if bytes.len() as u64 > limit {
            return Err(self.read_error(
                path,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("file exceeds {limit} bytes"),
                ),
            ));
        }
        Ok(bytes)
    }

    fn read_error(&self, path: &str, source: io::Error) -> ProjectCheckError {
        ProjectCheckError::Read {
            path: self.root.join(path),
            source,
        }
    }
}

fn invalid_source(
    path: impl Into<String>,
    source: impl Error + Send + Sync + 'static,
) -> ProjectCheckError {
    ProjectCheckError::Source {
        path: path.into(),
        source: Box::new(source),
    }
}
