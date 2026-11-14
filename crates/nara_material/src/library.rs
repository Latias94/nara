//! Material-local Shader publication and immutable generation custody.

use std::{collections::BTreeMap, sync::Arc};

use nara_asset::{
    AssetEvents, AssetMutationBaseline, AssetPath, AssetStateError, AssetStates, Assets, Handle,
    allocate_anonymous_handle,
};
use nara_ecs::{Component, Resource};
use nara_image::ImageAsset;

use crate::{
    Material2dDeclaration, MaterialDeclarationError, MaterialParameterValues, MaterialParameters,
    MaterialShaderArtifact, MaterialShaderError,
};

const MAX_DECLARATIONS: usize = 64;
const MAX_INPUTS: usize = 32;
const MAX_SOURCE_BYTES: usize = 1024 * 1024;

/// One source actually returned by the caller's scoped reader before Shader assembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialShaderInput {
    path: AssetPath,
    source: Arc<str>,
    digest: [u8; 32],
}

impl MaterialShaderInput {
    fn new(path: AssetPath, source: Arc<str>) -> Self {
        Self {
            path,
            digest: *blake3::hash(source.as_bytes()).as_bytes(),
            source,
        }
    }

    #[must_use]
    pub fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

/// A read attempt; an unavailable source has no content digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialShaderInputObservation {
    path: AssetPath,
    digest: Option<[u8; 32]>,
}

impl MaterialShaderInputObservation {
    #[must_use]
    pub fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub fn digest(&self) -> Option<[u8; 32]> {
        self.digest
    }
}

#[derive(Debug)]
struct Generation {
    sequence: u64,
    declaration: Arc<Material2dDeclaration>,
    shader: MaterialShaderArtifact,
    inputs: Box<[MaterialShaderInput]>,
}

/// One exact admitted declaration, binding ABI, Shader artifact and source cut.
///
/// Clones retain this generation after another is published or the library is dropped. Sequence
/// numbers are local observations; only the original prepared update can publish into a library.
#[derive(Debug, Clone)]
pub struct Material2dGeneration(Arc<Generation>);

impl PartialEq for Material2dGeneration {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Material2dGeneration {}

impl Material2dGeneration {
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.0.sequence
    }

    #[must_use]
    pub fn declaration(&self) -> &Material2dDeclaration {
        &self.0.declaration
    }

    #[must_use]
    pub fn shader(&self) -> &MaterialShaderArtifact {
        &self.0.shader
    }

    #[must_use]
    pub fn inputs(&self) -> &[MaterialShaderInput] {
        &self.0.inputs
    }

    /// Joins typed values to this exact declaration through the dynamic-value encoder.
    pub fn bind<T: MaterialParameters>(
        &self,
        parameters: &T,
        textures: BTreeMap<String, Handle<ImageAsset>>,
    ) -> Result<Material2dBinding, MaterialUpdateError> {
        if T::parameter_layout()? != *self.declaration().parameters() {
            return Err(MaterialUpdateError::ParameterLayout);
        }
        let binding = self.bind_values(parameters.parameter_values()?, textures)?;
        if parameters.encode_uniform()? != binding.uniform.as_ref() {
            return Err(MaterialUpdateError::ParameterEncoding);
        }
        Ok(binding)
    }

    /// Resolves stable semantic field IDs without consulting the current library generation.
    pub fn bind_values(
        &self,
        parameters: MaterialParameterValues,
        textures: BTreeMap<String, Handle<ImageAsset>>,
    ) -> Result<Material2dBinding, MaterialUpdateError> {
        let declaration = self.declaration();
        if textures.len() != declaration.textures().len()
            || declaration
                .textures()
                .iter()
                .any(|field| !textures.contains_key(field.id()))
        {
            return Err(MaterialUpdateError::TextureFields);
        }
        let uniform = declaration.parameters().encode(&parameters)?;
        Ok(Material2dBinding {
            generation: self.clone(),
            parameters,
            uniform: uniform.into(),
            textures,
        })
    }
}

/// Immutable per-draw values and typed resource references bound to one admitted generation.
///
/// The consuming render domain resolves handles to exact prepared Image captures; this value
/// does not allocate native resources or make a GPU submission.
#[derive(Debug, Clone, PartialEq, Component)]
pub struct Material2dBinding {
    generation: Material2dGeneration,
    parameters: MaterialParameterValues,
    uniform: Arc<[u8]>,
    textures: BTreeMap<String, Handle<ImageAsset>>,
}

impl Material2dBinding {
    #[must_use]
    pub fn generation(&self) -> &Material2dGeneration {
        &self.generation
    }

    #[must_use]
    pub fn parameters(&self) -> &MaterialParameterValues {
        &self.parameters
    }

    #[must_use]
    pub fn uniform_bytes(&self) -> &[u8] {
        &self.uniform
    }

    #[must_use]
    pub fn textures(&self) -> &BTreeMap<String, Handle<ImageAsset>> {
        &self.textures
    }
}

#[derive(Debug)]
struct Entry {
    declaration: Arc<Material2dDeclaration>,
    handle: Handle<Material2dGeneration>,
    next_sequence: u64,
    attempted_paths: Vec<AssetPath>,
    observations: Vec<MaterialShaderInputObservation>,
    failure: Option<MaterialUpdateError>,
}

/// A finite material-local publication owner, independent from native pipeline caches.
///
/// Source readers remain caller-owned and capability scoped. Ordered WGSL fragments are an
/// explicit source manifest, not a parsed import language. Failed/missing reads participate in
/// dependency observation; a failed edit never removes a current compatible generation.
#[derive(Debug, Resource)]
pub struct Material2dLibrary {
    identity: Arc<()>,
    entries: BTreeMap<String, Entry>,
    assets: Assets<Material2dGeneration>,
    states: AssetStates,
    events: AssetEvents,
}

impl Default for Material2dLibrary {
    fn default() -> Self {
        Self {
            identity: Arc::new(()),
            entries: BTreeMap::new(),
            assets: Assets::default(),
            states: AssetStates::default(),
            events: AssetEvents::default(),
        }
    }
}

/// An effect-free compiled edit retaining its exact expected current generation and library.
#[derive(Debug, Clone)]
pub struct PreparedMaterial2dUpdate {
    owner: Arc<()>,
    baseline: AssetMutationBaseline<Material2dGeneration>,
    candidate: Material2dGeneration,
}

impl PreparedMaterial2dUpdate {
    #[must_use]
    pub fn candidate(&self) -> &Material2dGeneration {
        &self.candidate
    }
}

impl Material2dLibrary {
    pub fn declare(
        &mut self,
        declaration: Material2dDeclaration,
    ) -> Result<(), MaterialUpdateError> {
        if self.entries.contains_key(declaration.id()) {
            return Err(MaterialUpdateError::DuplicateDeclaration(
                declaration.id().to_owned(),
            ));
        }
        if self.entries.len() == MAX_DECLARATIONS {
            return Err(MaterialUpdateError::DeclarationLimit);
        }
        let handle = allocate_anonymous_handle()
            .map_err(|error| MaterialUpdateError::Storage(error.to_string()))?;
        self.states.set_loading(handle.id());
        self.entries.insert(
            declaration.id().to_owned(),
            Entry {
                declaration: Arc::new(declaration),
                handle,
                next_sequence: 1,
                attempted_paths: Vec::new(),
                observations: Vec::new(),
                failure: None,
            },
        );
        Ok(())
    }

    #[must_use]
    pub fn current(&self, id: &str) -> Option<&Material2dGeneration> {
        self.assets.get(self.entries.get(id)?.handle)
    }

    #[must_use]
    pub fn last_failure(&self, id: &str) -> Option<&MaterialUpdateError> {
        self.entries.get(id)?.failure.as_ref()
    }

    #[must_use]
    pub fn observations(&self, id: &str) -> Option<&[MaterialShaderInputObservation]> {
        Some(&self.entries.get(id)?.observations)
    }

    /// Selects dependencies in the current or latest attempted source cut.
    #[must_use]
    pub fn affected_by(&self, path: &AssetPath) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(_, entry)| {
                entry.attempted_paths.contains(path)
                    || self.assets.get(entry.handle).is_some_and(|generation| {
                        generation.inputs().iter().any(|input| input.path() == path)
                    })
            })
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Compiles an explicitly ordered WGSL source closure without changing current publication.
    ///
    /// The reader returns actual immutable source bytes and owns path authorization and physical
    /// I/O. No ambient filesystem or hidden include parser is used here.
    pub fn prepare_update(
        &mut self,
        id: &str,
        paths: &[AssetPath],
        mut read: impl FnMut(&AssetPath) -> Result<Arc<str>, String>,
    ) -> Result<PreparedMaterial2dUpdate, MaterialUpdateError> {
        validate_paths(paths)?;
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| MaterialUpdateError::UnknownDeclaration(id.to_owned()))?;
        let baseline = self.assets.prepare_mutation(entry.handle, &self.states)?;
        entry.attempted_paths = paths.to_vec();
        entry.observations.clear();
        match prepare(entry, paths, &mut read) {
            Ok(candidate) => Ok(PreparedMaterial2dUpdate {
                owner: Arc::clone(&self.identity),
                baseline,
                candidate,
            }),
            Err(error) => {
                baseline.record_failure(
                    &mut self.assets,
                    &mut self.states,
                    &mut self.events,
                    error.to_string(),
                )?;
                entry.failure = Some(error.clone());
                Err(error)
            }
        }
    }

    /// Publishes into the original baseline after re-reading the captured source cut.
    ///
    /// A stale/foreign candidate performs no reads or current publication. Source drift or read
    /// failure preserves the candidate and compatible last-good for explicit retry.
    pub fn publish(
        &mut self,
        prepared: &PreparedMaterial2dUpdate,
        mut read: impl FnMut(&AssetPath) -> Result<Arc<str>, String>,
    ) -> Result<Material2dGeneration, MaterialUpdateError> {
        if !Arc::ptr_eq(&self.identity, &prepared.owner) {
            return Err(MaterialUpdateError::ForeignUpdate);
        }
        let id = prepared.candidate.declaration().id();
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| MaterialUpdateError::UnknownDeclaration(id.to_owned()))?;
        if !Arc::ptr_eq(&entry.declaration, &prepared.candidate.0.declaration) {
            return Err(MaterialUpdateError::StaleUpdate);
        }
        prepared
            .baseline
            .validate(&self.assets, &self.states)
            .map_err(|_| MaterialUpdateError::StaleUpdate)?;
        let mut bytes = 0_usize;
        for input in prepared.candidate.inputs() {
            let source =
                read(input.path()).map_err(|message| MaterialUpdateError::SourceUnavailable {
                    path: input.path.clone(),
                    message,
                })?;
            bytes = bytes
                .checked_add(source.len())
                .ok_or(MaterialUpdateError::SourceLimit)?;
            if bytes > MAX_SOURCE_BYTES {
                return Err(MaterialUpdateError::SourceLimit);
            }
            if *blake3::hash(source.as_bytes()).as_bytes() != input.digest {
                return Err(MaterialUpdateError::SourceChanged(input.path.clone()));
            }
        }
        prepared.baseline.clone().commit_loaded(
            prepared.candidate.clone(),
            &mut self.assets,
            &mut self.states,
            &mut self.events,
            None,
            None,
        )?;
        entry.attempted_paths.clear();
        entry.observations = prepared
            .candidate
            .inputs()
            .iter()
            .map(|input| MaterialShaderInputObservation {
                path: input.path.clone(),
                digest: Some(input.digest),
            })
            .collect();
        entry.failure = None;
        Ok(prepared.candidate.clone())
    }
}

fn validate_paths(paths: &[AssetPath]) -> Result<(), MaterialUpdateError> {
    if paths.is_empty() || paths.len() > MAX_INPUTS {
        return Err(MaterialUpdateError::SourceLimit);
    }
    for (index, path) in paths.iter().enumerate() {
        if paths[..index].contains(path) {
            return Err(MaterialUpdateError::DuplicateSource(path.clone()));
        }
    }
    Ok(())
}

fn prepare(
    entry: &mut Entry,
    paths: &[AssetPath],
    read: &mut impl FnMut(&AssetPath) -> Result<Arc<str>, String>,
) -> Result<Material2dGeneration, MaterialUpdateError> {
    let sequence = entry.next_sequence;
    let next_sequence = sequence
        .checked_add(1)
        .ok_or(MaterialUpdateError::GenerationExhausted)?;
    let mut inputs = Vec::with_capacity(paths.len());
    let mut bytes = 0_usize;
    for path in paths {
        let source = read(path).map_err(|message| {
            entry.observations.push(MaterialShaderInputObservation {
                path: path.clone(),
                digest: None,
            });
            MaterialUpdateError::SourceUnavailable {
                path: path.clone(),
                message,
            }
        })?;
        bytes = bytes
            .checked_add(source.len())
            .ok_or(MaterialUpdateError::SourceLimit)?;
        if bytes > MAX_SOURCE_BYTES {
            return Err(MaterialUpdateError::SourceLimit);
        }
        let input = MaterialShaderInput::new(path.clone(), source);
        entry.observations.push(MaterialShaderInputObservation {
            path: path.clone(),
            digest: Some(input.digest),
        });
        inputs.push(input);
    }
    let mut assembled = String::with_capacity(bytes.saturating_add(inputs.len()));
    for input in &inputs {
        assembled.push_str(input.source());
        assembled.push('\n');
    }
    let shader = MaterialShaderArtifact::compile(&entry.declaration, &assembled)?;
    entry.next_sequence = next_sequence;
    Ok(Material2dGeneration(Arc::new(Generation {
        sequence,
        declaration: Arc::clone(&entry.declaration),
        shader,
        inputs: inputs.into_boxed_slice(),
    })))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MaterialUpdateError {
    #[error(transparent)]
    Asset(#[from] AssetStateError),
    #[error("material storage admission failed: {0}")]
    Storage(String),
    #[error(transparent)]
    Declaration(#[from] MaterialDeclarationError),
    #[error(transparent)]
    Shader(#[from] MaterialShaderError),
    #[error("material declaration {0} already exists")]
    DuplicateDeclaration(String),
    #[error("material declaration {0} is unknown")]
    UnknownDeclaration(String),
    #[error("material declarations exceed the finite library limit")]
    DeclarationLimit,
    #[error("material Shader source closure exceeds its finite input/byte limit")]
    SourceLimit,
    #[error("material Shader input {0:?} appears more than once")]
    DuplicateSource(AssetPath),
    #[error("material Shader input {path:?} is unavailable: {message}")]
    SourceUnavailable { path: AssetPath, message: String },
    #[error("material Shader input {0:?} changed after preparation")]
    SourceChanged(AssetPath),
    #[error("material generation sequence is exhausted")]
    GenerationExhausted,
    #[error("material update belongs to another library")]
    ForeignUpdate,
    #[error("material update no longer matches its original publication baseline")]
    StaleUpdate,
    #[error("typed parameters do not match the captured material layout")]
    ParameterLayout,
    #[error("typed and dynamic parameter encoding disagree")]
    ParameterEncoding,
    #[error("material textures do not match the complete declared field set")]
    TextureFields,
}
