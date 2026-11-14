//! Material-local Shader publication and immutable generation custody.

use std::{any::Any, collections::BTreeMap, fmt, sync::Arc};

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
#[derive(Clone)]
pub struct MaterialShaderInput {
    path: AssetPath,
    source: Arc<str>,
    digest: [u8; 32],
    retention: Option<Arc<dyn Any + Send + Sync>>,
}

impl fmt::Debug for MaterialShaderInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MaterialShaderInput")
            .field("path", &self.path)
            .field("source", &self.source)
            .field("digest", &self.digest)
            .field("retained", &self.retention.is_some())
            .finish()
    }
}

impl PartialEq for MaterialShaderInput {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.source == other.source && self.digest == other.digest
    }
}

impl Eq for MaterialShaderInput {}

impl MaterialShaderInput {
    fn new(path: AssetPath, source: Arc<str>) -> Self {
        Self {
            path,
            digest: *blake3::hash(source.as_bytes()).as_bytes(),
            source,
            retention: None,
        }
    }

    fn with_lifetime_owner(&self, owner: Arc<dyn Any + Send + Sync>) -> Self {
        Self {
            path: self.path.clone(),
            source: Arc::clone(&self.source),
            digest: self.digest,
            retention: Some(owner),
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
#[derive(Clone)]
pub struct Material2dGeneration {
    inner: Arc<Generation>,
    retention: Option<Arc<GenerationLifetimeView>>,
}

struct GenerationLifetimeView {
    owner: Arc<dyn Any + Send + Sync>,
    shader: MaterialShaderArtifact,
    inputs: Box<[MaterialShaderInput]>,
}

struct GenerationLifetimeOwner<T> {
    _previous: Option<Arc<dyn Any + Send + Sync>>,
    _owner: Arc<T>,
}

impl fmt::Debug for Material2dGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Material2dGeneration")
            .field("inner", &self.inner)
            .field("retained", &self.retention.is_some())
            .finish()
    }
}

impl PartialEq for Material2dGeneration {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for Material2dGeneration {}

impl std::hash::Hash for Material2dGeneration {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&Arc::as_ptr(&self.inner), state);
    }
}

impl Material2dGeneration {
    /// Retains this exact generation, its previous owner and an additional opaque lifetime owner.
    ///
    /// This extends ownership without admitting a budget, source receipt or publication. The new
    /// owner also follows owned Shader and input snapshots. A separate read-only view shares the
    /// original source allocations; the immutable generation never points back to an owner or
    /// view. A content Host can retain its original, unwrapped acquisition without a cycle.
    #[must_use]
    pub fn with_lifetime_owner<T: Send + Sync + 'static>(&self, owner: Arc<T>) -> Self {
        let retention: Arc<dyn Any + Send + Sync> = Arc::new(GenerationLifetimeOwner {
            _previous: self.retention.as_ref().map(|view| Arc::clone(&view.owner)),
            _owner: owner,
        });
        let shader = self
            .inner
            .shader
            .with_lifetime_owner(Arc::clone(&retention));
        let inputs = self
            .inner
            .inputs
            .iter()
            .map(|input| input.with_lifetime_owner(Arc::clone(&retention)))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            inner: Arc::clone(&self.inner),
            retention: Some(Arc::new(GenerationLifetimeView {
                owner: retention,
                shader,
                inputs,
            })),
        }
    }

    /// Modeled out-of-line bytes allocated by one call to [`Self::with_lifetime_owner`].
    ///
    /// This includes the owner link, shared view, input array and cloned paths. Shader and input
    /// source allocations are shared. The caller separately owns the supplied acquisition's
    /// charge, any previous owner's charge and the inline cloned value. Returns `None` on overflow.
    #[must_use]
    pub fn lifetime_owner_retained_bytes(&self) -> Option<usize> {
        use std::mem::{size_of, size_of_val};

        let arc_counters = 2 * size_of::<usize>();
        let bytes = size_of::<GenerationLifetimeOwner<()>>()
            .checked_add(arc_counters)?
            .checked_add(size_of::<GenerationLifetimeView>())?
            .checked_add(arc_counters)?
            .checked_add(size_of_val(self.inner.inputs.as_ref()))?;
        self.inner.inputs.iter().try_fold(bytes, |bytes, input| {
            bytes.checked_add(input.path().as_str().len())
        })
    }

    /// Compiles an independent immutable generation without publication or source observations.
    ///
    /// Sequence zero identifies a value constructed outside the code-first library. The supplied
    /// body is compiled by the same stock assembly and Naga interface authority as library edits.
    pub fn from_shader_source(
        declaration: Material2dDeclaration,
        body: &str,
    ) -> Result<Self, MaterialUpdateError> {
        let shader = MaterialShaderArtifact::compile(&declaration, body)?;
        Ok(Self {
            inner: Arc::new(Generation {
                sequence: 0,
                declaration: Arc::new(declaration.clone()),
                shader,
                inputs: Box::new([]),
            }),
            retention: None,
        })
    }

    /// Compiles an ordered set of actual caller-read inputs without publishing a current value.
    pub fn from_shader_inputs(
        declaration: Material2dDeclaration,
        inputs: Vec<(AssetPath, Arc<str>)>,
    ) -> Result<Self, MaterialUpdateError> {
        if inputs.is_empty() || inputs.len() > MAX_INPUTS {
            return Err(MaterialUpdateError::SourceLimit);
        }
        inputs
            .iter()
            .try_fold(inputs.len(), |bytes, (_, source)| {
                bytes
                    .checked_add(source.len())
                    .filter(|bytes| *bytes <= MAX_SOURCE_BYTES)
            })
            .ok_or(MaterialUpdateError::SourceLimit)?;
        let paths: Vec<_> = inputs.iter().map(|(path, _)| path.clone()).collect();
        validate_paths(&paths)?;
        let inputs = inputs
            .into_iter()
            .map(|(path, source)| MaterialShaderInput::new(path.clone(), source))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        compile_inputs(0, Arc::new(declaration.clone()), inputs)
    }

    #[cfg(feature = "asset")]
    pub(crate) fn product_retained_bytes(&self) -> Option<usize> {
        use std::mem::size_of;

        if self.sequence() != 0 || self.retention.is_some() {
            return None;
        }
        let declaration = self.declaration();
        let arc_counters = 2 * size_of::<usize>();
        let mut bytes = size_of::<Generation>()
            .checked_add(arc_counters)?
            .checked_add(size_of::<Material2dDeclaration>())?
            .checked_add(arc_counters)?
            .checked_add(declaration.id().len())?
            .checked_add(std::mem::size_of_val(declaration.parameters().fields()))?
            .checked_add(std::mem::size_of_val(declaration.textures()))?;
        for field in declaration.parameters().fields() {
            bytes = bytes
                .checked_add(field.id().len())?
                .checked_add(field.name().len())?;
        }
        for texture in declaration.textures() {
            bytes = bytes
                .checked_add(texture.id().len())?
                .checked_add(texture.name().len())?;
        }
        for source in [self.shader().user_source(), self.shader().source()] {
            bytes = bytes.checked_add(source.len())?.checked_add(arc_counters)?;
        }
        bytes = bytes.checked_add(std::mem::size_of_val(self.inputs()))?;
        for input in self.inputs() {
            bytes = bytes
                .checked_add(input.path().as_str().len())?
                .checked_add(input.source().len())?
                .checked_add(arc_counters)?;
        }
        Some(bytes)
    }

    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.inner.sequence
    }

    #[must_use]
    pub fn declaration(&self) -> &Material2dDeclaration {
        &self.inner.declaration
    }

    #[must_use]
    pub fn shader(&self) -> &MaterialShaderArtifact {
        self.retention
            .as_ref()
            .map_or(&self.inner.shader, |view| &view.shader)
    }

    #[must_use]
    pub fn inputs(&self) -> &[MaterialShaderInput] {
        self.retention
            .as_ref()
            .map_or(self.inner.inputs.as_ref(), |view| view.inputs.as_ref())
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
        if !Arc::ptr_eq(&entry.declaration, &prepared.candidate.inner.declaration) {
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
    let generation = compile_inputs(
        sequence,
        Arc::clone(&entry.declaration),
        inputs.into_boxed_slice(),
    )?;
    entry.next_sequence = next_sequence;
    Ok(generation)
}

fn compile_inputs(
    sequence: u64,
    declaration: Arc<Material2dDeclaration>,
    inputs: Box<[MaterialShaderInput]>,
) -> Result<Material2dGeneration, MaterialUpdateError> {
    let bytes = inputs
        .iter()
        .try_fold(inputs.len(), |bytes, input| {
            bytes
                .checked_add(input.source().len())
                .filter(|bytes| *bytes <= MAX_SOURCE_BYTES)
        })
        .ok_or(MaterialUpdateError::SourceLimit)?;
    let mut assembled = String::with_capacity(bytes);
    for input in inputs.iter() {
        assembled.push_str(input.source());
        assembled.push('\n');
    }
    let shader = MaterialShaderArtifact::compile(&declaration, &assembled)?;
    Ok(Material2dGeneration {
        inner: Arc::new(Generation {
            sequence,
            declaration,
            shader,
            inputs,
        }),
        retention: None,
    })
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
