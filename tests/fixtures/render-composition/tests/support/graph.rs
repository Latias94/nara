//! Disposable, private resource-dependency comparison. This is not an engine API or a GPU owner.
//!
//! Native descriptors are exact wgpu 30 types. The admitted scope is whole, single-sample 2D
//! color textures and whole, unmapped, four-byte-aligned buffers. The executor must allocate
//! distinct storage for every local version, perform every write's initialization before its
//! operation, and retain imports externally. A descriptor or an initialized-import assertion
//! does not authenticate physical custody, device capabilities, shader bindings or GPU completion.
//!
//! All graph-owned collection capacities have a checked finite envelope before allocation.
//! Modeled metadata counts Rust values and compile scratch; modeled resource bytes count texels
//! and buffer lengths. Neither is RSS, allocator overhead, native heap size or a GPU reservation.
//! Input slices and borrowed static descriptor labels stay in their caller's custody.

use std::{fmt, mem::size_of, sync::Arc};

pub(super) type OperationKey = u64;

#[derive(Clone)]
pub(super) struct Version {
    owner: Arc<()>,
    index: usize,
}

impl Version {
    /// An executor-local table index, meaningful only after candidate ownership validation.
    pub(super) fn index(&self) -> usize {
        self.index
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && Arc::ptr_eq(&self.owner, &other.owner)
    }
}

impl Eq for Version {}

impl fmt::Debug for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Version")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ColorTransfer {
    Linear,
    Srgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AlphaMode {
    Opaque,
    Straight,
    Premultiplied,
}

/// Code-owned wiring, independent of physical format and of operation registration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextureMeaning {
    pub(super) semantic: u64,
    pub(super) transfer: ColorTransfer,
    pub(super) alpha: AlphaMode,
}

/// Code-owned buffer port meaning, independent of native size and usage flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BufferMeaning {
    pub(super) semantic: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ResourceDesc {
    Texture {
        native: wgpu::TextureDescriptor<'static>,
        range: wgpu::ImageSubresourceRange,
        meaning: TextureMeaning,
    },
    Buffer {
        native: wgpu::BufferDescriptor<'static>,
        meaning: BufferMeaning,
    },
}

impl ResourceDesc {
    fn validate(&self, limits: Limits) -> Result<u64, GraphError> {
        match self {
            Self::Texture { native, range, .. } => {
                if native.dimension != wgpu::TextureDimension::D2
                    || native.size.width == 0
                    || native.size.height == 0
                    || native.size.depth_or_array_layers != 1
                    || native.sample_count != 1
                    || native.mip_level_count != 1
                    || !native.view_formats.is_empty()
                {
                    return Err(GraphError::UnsupportedTextureScope);
                }
                if native.size.width > limits.max_texture_dimension_2d
                    || native.size.height > limits.max_texture_dimension_2d
                {
                    return Err(GraphError::LimitExceeded(LimitKind::TextureDimension));
                }
                // Keep optional formats, planes, depth/stencil and compressed blocks outside
                // this experiment. Actual device/adapter admission still belongs to the Host.
                if !native.format.required_features().is_empty()
                    || native.format.is_depth_stencil_format()
                    || native.format.block_dimensions() != (1, 1)
                {
                    return Err(GraphError::UnsupportedTextureFormat);
                }
                if !range.is_full_resource(native.format, 1, 1) {
                    return Err(GraphError::UnsupportedTextureScope);
                }
                let supported = wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST;
                let format = native
                    .format
                    .guaranteed_format_features(wgpu::Features::empty());
                if native.usage.is_empty()
                    || !supported.contains(native.usage)
                    || !format.allowed_usages.contains(native.usage)
                {
                    return Err(GraphError::UnsupportedUsage);
                }
                let texel_bytes = native
                    .format
                    .block_copy_size(None)
                    .ok_or(GraphError::UnsupportedTextureFormat)?;
                u64::from(native.size.width)
                    .checked_mul(u64::from(native.size.height))
                    .and_then(|pixels| pixels.checked_mul(u64::from(texel_bytes)))
                    .ok_or(GraphError::LimitExceeded(LimitKind::ResourceBytes))
            }
            Self::Buffer { native, .. } => {
                if native.size == 0
                    || native.size % wgpu::COPY_BUFFER_ALIGNMENT != 0
                    || native.mapped_at_creation
                {
                    return Err(GraphError::UnsupportedBufferScope);
                }
                if native.size > limits.max_buffer_bytes {
                    return Err(GraphError::LimitExceeded(LimitKind::BufferBytes));
                }
                let supported = wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::UNIFORM
                    | wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::INDEX
                    | wgpu::BufferUsages::INDIRECT;
                if native.usage.is_empty() || !supported.contains(native.usage) {
                    return Err(GraphError::UnsupportedUsage);
                }
                Ok(native.size)
            }
        }
    }

    /// Initialization copies preserve physical shape and color encoding, not the output port's
    /// semantic. The operation may turn copied scene color into a composite or counts into floats.
    fn initialization_copy_compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Texture {
                    native: left,
                    meaning: left_meaning,
                    ..
                },
                Self::Texture {
                    native: right,
                    meaning: right_meaning,
                    ..
                },
            ) => {
                left.size == right.size
                    && left.format == right.format
                    && left.dimension == right.dimension
                    && left.sample_count == right.sample_count
                    && left.mip_level_count == right.mip_level_count
                    && left_meaning.transfer == right_meaning.transfer
                    && left_meaning.alpha == right_meaning.alpha
            }
            (Self::Buffer { native: left, .. }, Self::Buffer { native: right, .. }) => {
                left.size == right.size
            }
            _ => false,
        }
    }

    /// Reads require the exact port meaning as well as compatible shape and native usages.
    fn satisfies(&self, expected: &Self) -> bool {
        self.initialization_copy_compatible(expected)
            && match (self, expected) {
                (
                    Self::Texture {
                        native: actual,
                        meaning: actual_meaning,
                        ..
                    },
                    Self::Texture {
                        native: wanted,
                        meaning: wanted_meaning,
                        ..
                    },
                ) => actual_meaning == wanted_meaning && actual.usage.contains(wanted.usage),
                (
                    Self::Buffer {
                        native: actual,
                        meaning: actual_meaning,
                    },
                    Self::Buffer {
                        native: wanted,
                        meaning: wanted_meaning,
                    },
                ) => actual_meaning == wanted_meaning && actual.usage.contains(wanted.usage),
                _ => false,
            }
    }
}

/// Dependency roles, not a replacement for native binding/pipeline descriptors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AccessRole {
    Sampled,
    Attachment,
    StorageRead,
    StorageWrite,
    StorageReadWrite,
    CopySource,
    CopyDestination,
    Uniform,
    Vertex,
    Index,
    Indirect,
}

impl AccessRole {
    fn is_read(self) -> bool {
        matches!(
            self,
            Self::Sampled
                | Self::StorageRead
                | Self::CopySource
                | Self::Uniform
                | Self::Vertex
                | Self::Index
                | Self::Indirect
        )
    }

    fn is_write(self) -> bool {
        matches!(
            self,
            Self::Attachment | Self::StorageWrite | Self::StorageReadWrite | Self::CopyDestination
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Read {
    pub(super) version: Version,
    pub(super) role: AccessRole,
    pub(super) expected: ResourceDesc,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Initialization {
    /// A whole-attachment clear, including outside the operation's viewport/scissor.
    Clear(wgpu::Color),
    /// A whole-buffer clear through COPY_DST, not an assumption about allocator contents.
    Zero,
    /// A whole-resource copy into distinct storage before the operation executes. The successor
    /// may have a different port semantic; texture transfer and alpha encoding remain unchanged.
    Preserve(Version),
}

impl Initialization {
    fn predecessor(&self) -> Option<&Version> {
        match self {
            Self::Preserve(version) => Some(version),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Write {
    pub(super) version: Version,
    pub(super) role: AccessRole,
    pub(super) initialization: Initialization,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImportState {
    Initialized,
    Uninitialized,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LimitKind {
    Resources,
    Operations,
    Accesses,
    Outputs,
    TextureDimension,
    BufferBytes,
    ResourceBytes,
    MetadataBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GraphError {
    InvalidLimits,
    LimitExceeded(LimitKind),
    AllocationFailed,
    ForeignCandidate,
    UnknownResource(usize),
    UnsupportedTextureScope,
    UnsupportedTextureFormat,
    UnsupportedBufferScope,
    UnsupportedUsage,
    UninitializedImport,
    DuplicateOperationKey(OperationKey),
    IllegalAccess(AccessRole),
    IllegalKind(AccessRole),
    MissingUsage { version: usize, role: AccessRole },
    IncompatibleRead(usize),
    IncompatiblePreserve(usize),
    InvalidInitialization(usize),
    ImportedWrite(usize),
    RepeatedProducer(usize),
    ReadWriteAlias(usize),
    MissingProducer(usize),
    MissingOutputGroup,
    DuplicateOutput(usize),
    Cycle,
}

impl fmt::Display for GraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for GraphError {}

#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    pub(super) max_resources: usize,
    pub(super) max_operations: usize,
    /// Reads, writes, and one additional implicit copy-source access per Preserve.
    pub(super) max_accesses: usize,
    pub(super) max_outputs: usize,
    pub(super) max_texture_dimension_2d: u32,
    pub(super) max_buffer_bytes: u64,
    pub(super) max_modeled_resource_bytes: u64,
    pub(super) max_modeled_metadata_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_resources: 128,
            max_operations: 128,
            max_accesses: 512,
            max_outputs: 128,
            max_texture_dimension_2d: 64,
            max_buffer_bytes: 16 * 1024 * 1024,
            max_modeled_resource_bytes: 16 * 1024 * 1024,
            max_modeled_metadata_bytes: 1024 * 1024,
        }
    }
}

impl Limits {
    /// Conservative capacity envelope, including serial compilation scratch and the output group.
    /// Vec allocator rounding and caller-owned input allocations are intentionally not RSS claims.
    pub(super) fn modeled_metadata_bytes(self) -> Result<usize, GraphError> {
        if self.max_resources == 0
            || self.max_outputs == 0
            || self.max_texture_dimension_2d == 0
            || self.max_buffer_bytes == 0
            || self.max_modeled_resource_bytes == 0
        {
            return Err(GraphError::InvalidLimits);
        }
        let mut total =
            size_of::<Candidate>() + size_of::<CompiledGraph>() + 2 * size_of::<usize>(); // The shared Arc identity's strong/weak counts.
        for (count, bytes) in [
            (self.max_resources, size_of::<Resource>()),
            (self.max_operations, size_of::<Operation>()),
            (self.max_accesses, size_of::<Read>().max(size_of::<Write>())),
            (self.max_outputs, size_of::<Version>()),
            (self.max_operations, 2 * size_of::<usize>()), // Indegrees and serial order.
        ] {
            total = count
                .checked_mul(bytes)
                .and_then(|bytes| total.checked_add(bytes))
                .ok_or(GraphError::LimitExceeded(LimitKind::MetadataBytes))?;
        }
        if total > self.max_modeled_metadata_bytes {
            return Err(GraphError::LimitExceeded(LimitKind::MetadataBytes));
        }
        Ok(total)
    }
}

#[derive(Clone, Copy, Debug)]
enum Origin {
    Imported,
    Local { producer: Option<usize> },
}

impl Origin {
    fn producer(self) -> Option<usize> {
        match self {
            Self::Imported => None,
            Self::Local { producer } => producer,
        }
    }
}

#[derive(Debug)]
pub(super) struct Resource {
    pub(super) version: Version,
    pub(super) desc: ResourceDesc,
    origin: Origin,
}

impl Resource {
    pub(super) fn is_imported(&self) -> bool {
        matches!(self.origin, Origin::Imported)
    }
}

#[derive(Debug)]
pub(super) struct Operation {
    pub(super) key: OperationKey,
    pub(super) reads: Vec<Read>,
    pub(super) writes: Vec<Write>,
}

impl Operation {
    fn dependencies(&self) -> impl Iterator<Item = &Version> {
        self.reads.iter().map(|read| &read.version).chain(
            self.writes
                .iter()
                .filter_map(|write| write.initialization.predecessor()),
        )
    }
}

#[derive(Debug)]
pub(super) struct Candidate {
    owner: Arc<()>,
    limits: Limits,
    resources: Vec<Resource>,
    operations: Vec<Operation>,
    outputs: Vec<Version>,
    accesses: usize,
    modeled_metadata_bytes: usize,
    modeled_resource_bytes: u64,
}

impl Candidate {
    pub(super) fn new(limits: Limits) -> Result<Self, GraphError> {
        // Check every capacity and the complete metadata envelope before any graph allocation.
        let modeled_metadata_bytes = limits.modeled_metadata_bytes()?;
        Ok(Self {
            owner: Arc::new(()),
            limits,
            resources: reserved(limits.max_resources)?,
            operations: reserved(limits.max_operations)?,
            outputs: reserved(limits.max_outputs)?,
            accesses: 0,
            modeled_metadata_bytes,
            modeled_resource_bytes: 0,
        })
    }

    pub(super) fn reserve_resource(&mut self, desc: ResourceDesc) -> Result<Version, GraphError> {
        self.add_resource(desc, Origin::Local { producer: None })
    }

    /// The Host asserts complete initialization and retains the exact physical input separately.
    /// No existing candidate handle is accepted here, and no import may become a local write.
    pub(super) fn import_resource(
        &mut self,
        desc: ResourceDesc,
        state: ImportState,
    ) -> Result<Version, GraphError> {
        if state != ImportState::Initialized {
            return Err(GraphError::UninitializedImport);
        }
        self.add_resource(desc, Origin::Imported)
    }

    fn add_resource(&mut self, desc: ResourceDesc, origin: Origin) -> Result<Version, GraphError> {
        if self.resources.len() == self.limits.max_resources {
            return Err(GraphError::LimitExceeded(LimitKind::Resources));
        }
        let bytes = desc.validate(self.limits)?;
        let modeled_resource_bytes = self
            .modeled_resource_bytes
            .checked_add(bytes)
            .filter(|bytes| *bytes <= self.limits.max_modeled_resource_bytes)
            .ok_or(GraphError::LimitExceeded(LimitKind::ResourceBytes))?;
        let version = Version {
            owner: Arc::clone(&self.owner),
            index: self.resources.len(),
        };
        self.resources.push(Resource {
            version: version.clone(),
            desc,
            origin,
        });
        self.modeled_resource_bytes = modeled_resource_bytes;
        Ok(version)
    }

    fn resource(&self, version: &Version) -> Result<&Resource, GraphError> {
        resource(&self.owner, &self.resources, version)
    }

    /// Input slices remain caller-owned, including on rejection. Validation and count admission
    /// precede graph-owned copies. A rejected operation changes no producer or allowance.
    pub(super) fn add_operation(
        &mut self,
        key: OperationKey,
        reads: &[Read],
        writes: &[Write],
    ) -> Result<(), GraphError> {
        if self.operations.iter().any(|operation| operation.key == key) {
            return Err(GraphError::DuplicateOperationKey(key));
        }
        if self.operations.len() == self.limits.max_operations {
            return Err(GraphError::LimitExceeded(LimitKind::Operations));
        }
        let accesses = self
            .accesses
            .checked_add(reads.len())
            .and_then(|count| count.checked_add(writes.len()))
            .filter(|count| *count <= self.limits.max_accesses)
            .ok_or(GraphError::LimitExceeded(LimitKind::Accesses))?;
        let copies = writes
            .iter()
            .filter(|write| write.initialization.predecessor().is_some())
            .count();
        let accesses = accesses
            .checked_add(copies)
            .filter(|count| *count <= self.limits.max_accesses)
            .ok_or(GraphError::LimitExceeded(LimitKind::Accesses))?;

        for read in reads {
            let actual = &self.resource(&read.version)?.desc;
            validate_access(actual, &read.version, read.role, false)?;
            read.expected.validate(self.limits)?;
            if !actual.satisfies(&read.expected) {
                return Err(GraphError::IncompatibleRead(read.version.index));
            }
            if writes.iter().any(|write| write.version == read.version) {
                return Err(GraphError::ReadWriteAlias(read.version.index));
            }
        }
        for (index, write) in writes.iter().enumerate() {
            let output = self.resource(&write.version)?;
            match output.origin {
                Origin::Imported => return Err(GraphError::ImportedWrite(write.version.index)),
                Origin::Local { producer: Some(_) } => {
                    return Err(GraphError::RepeatedProducer(write.version.index));
                }
                Origin::Local { producer: None } => {}
            }
            if writes[..index]
                .iter()
                .any(|other| other.version == write.version)
            {
                return Err(GraphError::RepeatedProducer(write.version.index));
            }
            validate_access(&output.desc, &write.version, write.role, true)?;
            match &write.initialization {
                Initialization::Clear(color) => {
                    if !matches!(output.desc, ResourceDesc::Texture { .. })
                        || ![color.r, color.g, color.b, color.a]
                            .into_iter()
                            .all(f64::is_finite)
                    {
                        return Err(GraphError::InvalidInitialization(write.version.index));
                    }
                    validate_access(&output.desc, &write.version, AccessRole::Attachment, true)?;
                }
                Initialization::Zero => {
                    if !matches!(output.desc, ResourceDesc::Buffer { .. }) {
                        return Err(GraphError::InvalidInitialization(write.version.index));
                    }
                    validate_access(
                        &output.desc,
                        &write.version,
                        AccessRole::CopyDestination,
                        true,
                    )?;
                }
                Initialization::Preserve(predecessor) => {
                    let input = self.resource(predecessor)?;
                    if writes.iter().any(|other| other.version == *predecessor) {
                        return Err(GraphError::ReadWriteAlias(predecessor.index));
                    }
                    if !output.desc.initialization_copy_compatible(&input.desc) {
                        return Err(GraphError::IncompatiblePreserve(write.version.index));
                    }
                    validate_access(&input.desc, predecessor, AccessRole::CopySource, false)?;
                    validate_access(
                        &output.desc,
                        &write.version,
                        AccessRole::CopyDestination,
                        true,
                    )?;
                }
            }
        }

        let mut admitted_reads = reserved(reads.len())?;
        let mut admitted_writes = reserved(writes.len())?;
        admitted_reads.extend_from_slice(reads);
        admitted_writes.extend_from_slice(writes);
        let producer = self.operations.len();
        for write in writes {
            self.resources[write.version.index].origin = Origin::Local {
                producer: Some(producer),
            };
        }
        self.operations.push(Operation {
            key,
            reads: admitted_reads,
            writes: admitted_writes,
        });
        self.accesses = accesses;
        Ok(())
    }

    /// An explicit complete output set. This declares no publication/history side effect.
    /// Replacing the set is atomic with respect to validation; an empty group is not admitted.
    pub(super) fn set_output_group(&mut self, outputs: &[Version]) -> Result<(), GraphError> {
        if outputs.is_empty() {
            return Err(GraphError::MissingOutputGroup);
        }
        if outputs.len() > self.limits.max_outputs {
            return Err(GraphError::LimitExceeded(LimitKind::Outputs));
        }
        for (index, output) in outputs.iter().enumerate() {
            self.resource(output)?;
            if outputs[..index].contains(output) {
                return Err(GraphError::DuplicateOutput(output.index));
            }
        }
        self.outputs.clear();
        self.outputs.extend_from_slice(outputs);
        Ok(())
    }

    pub(super) fn compile(self) -> Result<CompiledGraph, GraphError> {
        if self.outputs.is_empty() {
            return Err(GraphError::MissingOutputGroup);
        }
        for resource in &self.resources {
            if matches!(resource.origin, Origin::Local { producer: None }) {
                return Err(GraphError::MissingProducer(resource.version.index));
            }
        }
        let mut indegrees = reserved(self.operations.len())?;
        indegrees.resize(self.operations.len(), 0usize);
        let mut order = reserved(self.operations.len())?;
        for (index, operation) in self.operations.iter().enumerate() {
            indegrees[index] = operation
                .dependencies()
                .filter(|version| self.resources[version.index].origin.producer().is_some())
                .count();
        }
        // A bounded serial comparison needs no generic scheduler or adjacency allocation.
        // Each explicit read and implicit preservation read contributes one matching decrement.
        while order.len() < self.operations.len() {
            let next = (0..self.operations.len())
                .filter(|index| indegrees[*index] == 0)
                .min_by_key(|index| self.operations[*index].key)
                .ok_or(GraphError::Cycle)?;
            order.push(next);
            indegrees[next] = usize::MAX;
            for (index, operation) in self.operations.iter().enumerate() {
                if indegrees[index] == usize::MAX {
                    continue;
                }
                let completed = operation
                    .dependencies()
                    .filter(|version| self.resources[version.index].origin.producer() == Some(next))
                    .count();
                indegrees[index] -= completed;
            }
        }
        Ok(CompiledGraph {
            owner: self.owner,
            resources: self.resources,
            operations: self.operations,
            order,
            outputs: self.outputs,
            modeled_metadata_bytes: self.modeled_metadata_bytes,
            modeled_resource_bytes: self.modeled_resource_bytes,
        })
    }
}

#[derive(Debug)]
pub(super) struct CompiledGraph {
    owner: Arc<()>,
    resources: Vec<Resource>,
    operations: Vec<Operation>,
    order: Vec<usize>,
    outputs: Vec<Version>,
    modeled_metadata_bytes: usize,
    modeled_resource_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CopyDependency {
    pub(super) source: Version,
    pub(super) destination: Version,
    pub(super) before: OperationKey,
}

impl CompiledGraph {
    pub(super) fn resources(&self) -> &[Resource] {
        &self.resources
    }

    pub(super) fn resource(&self, version: &Version) -> Result<&Resource, GraphError> {
        resource(&self.owner, &self.resources, version)
    }

    /// Every write's clear/zero/copy must execute before this operation's kernel.
    pub(super) fn operations(&self) -> impl ExactSizeIterator<Item = &Operation> {
        self.order.iter().map(|index| &self.operations[*index])
    }

    pub(super) fn outputs(&self) -> &[Version] {
        &self.outputs
    }

    pub(super) fn copy_dependencies(&self) -> impl Iterator<Item = CopyDependency> + '_ {
        self.operations().flat_map(|operation| {
            operation.writes.iter().filter_map(move |write| {
                write
                    .initialization
                    .predecessor()
                    .map(|source| CopyDependency {
                        source: source.clone(),
                        destination: write.version.clone(),
                        before: operation.key,
                    })
            })
        })
    }

    pub(super) fn modeled_metadata_bytes(&self) -> usize {
        self.modeled_metadata_bytes
    }

    pub(super) fn modeled_resource_bytes(&self) -> u64 {
        self.modeled_resource_bytes
    }
}

fn resource<'a>(
    owner: &Arc<()>,
    resources: &'a [Resource],
    version: &Version,
) -> Result<&'a Resource, GraphError> {
    if !Arc::ptr_eq(owner, &version.owner) {
        return Err(GraphError::ForeignCandidate);
    }
    resources
        .get(version.index)
        .ok_or(GraphError::UnknownResource(version.index))
}

fn reserved<T>(count: usize) -> Result<Vec<T>, GraphError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| GraphError::AllocationFailed)?;
    Ok(values)
}

fn validate_access(
    desc: &ResourceDesc,
    version: &Version,
    role: AccessRole,
    write: bool,
) -> Result<(), GraphError> {
    if (write && !role.is_write()) || (!write && !role.is_read()) {
        return Err(GraphError::IllegalAccess(role));
    }
    let valid = match desc {
        ResourceDesc::Texture { native, .. } => {
            let usage = match role {
                AccessRole::Sampled => wgpu::TextureUsages::TEXTURE_BINDING,
                AccessRole::Attachment => wgpu::TextureUsages::RENDER_ATTACHMENT,
                AccessRole::StorageRead
                | AccessRole::StorageWrite
                | AccessRole::StorageReadWrite => wgpu::TextureUsages::STORAGE_BINDING,
                AccessRole::CopySource => wgpu::TextureUsages::COPY_SRC,
                AccessRole::CopyDestination => wgpu::TextureUsages::COPY_DST,
                _ => return Err(GraphError::IllegalKind(role)),
            };
            let format = native
                .format
                .guaranteed_format_features(wgpu::Features::empty());
            let storage = match role {
                AccessRole::StorageRead => wgpu::TextureFormatFeatureFlags::STORAGE_READ_ONLY,
                AccessRole::StorageWrite => wgpu::TextureFormatFeatureFlags::STORAGE_WRITE_ONLY,
                AccessRole::StorageReadWrite => wgpu::TextureFormatFeatureFlags::STORAGE_READ_WRITE,
                _ => wgpu::TextureFormatFeatureFlags::empty(),
            };
            if !format.flags.contains(storage) {
                return Err(GraphError::IllegalAccess(role));
            }
            native.usage.contains(usage)
        }
        ResourceDesc::Buffer { native, .. } => {
            let usage = match role {
                AccessRole::StorageRead
                | AccessRole::StorageWrite
                | AccessRole::StorageReadWrite => wgpu::BufferUsages::STORAGE,
                AccessRole::CopySource => wgpu::BufferUsages::COPY_SRC,
                AccessRole::CopyDestination => wgpu::BufferUsages::COPY_DST,
                AccessRole::Uniform => wgpu::BufferUsages::UNIFORM,
                AccessRole::Vertex => wgpu::BufferUsages::VERTEX,
                AccessRole::Index => wgpu::BufferUsages::INDEX,
                AccessRole::Indirect => wgpu::BufferUsages::INDIRECT,
                _ => return Err(GraphError::IllegalKind(role)),
            };
            native.usage.contains(usage)
        }
    };
    if !valid {
        return Err(GraphError::MissingUsage {
            version: version.index,
            role,
        });
    }
    Ok(())
}
