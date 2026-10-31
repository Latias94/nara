//! Finite resource declarations for the WGPU Host's execution candidates.
//!
//! This module owns logical admission, not device capabilities, native allocations or publication.
//! The Host keeps native operation payloads in its own table keyed by `GraphOperation::key`. It
//! allocates distinct storage for every local version, executes every write's full initialization
//! before its operation, and retains imports and submitted uses through physical completion.
//!
//! Shared textures are whole, single-sample, one-mip 2D color resources. Shared buffers are whole,
//! unmapped and four-byte aligned. Unsupported scopes reject rather than approximate their ranges.
//! All shared attachment writes must Store, including initialization clears and subsequent passes.
//! Native adapters remain trusted to obey these declarations; this is not a shader verifier or RHI.

use std::{any::TypeId, collections::BinaryHeap, fmt, mem::size_of, sync::Arc};

#[path = "compile.rs"]
mod compile;

/// A candidate-local version. Retaining the identity allocation prevents address-reuse confusion
/// even when its builder or compiled graph has already been dropped.
#[derive(Clone)]
pub(crate) struct ResourceVersion {
    owner: Arc<()>,
    index: usize,
}

impl ResourceVersion {
    /// Only a table index, never authority to import, write or publish a resource. Validate the
    /// version against its graph before using this index in the Host's physical-resource table.
    pub(crate) fn index(&self) -> usize {
        self.index
    }
}

impl PartialEq for ResourceVersion {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && Arc::ptr_eq(&self.owner, &other.owner)
    }
}

impl Eq for ResourceVersion {}

impl fmt::Debug for ResourceVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResourceVersion")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

/// Explicit content transfer interpretation carried by a texture port and its physical lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureTransfer {
    Linear,
    Srgb,
}

/// Alpha interpretation that must agree between a port and its exact texture version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureAlpha {
    Opaque,
    Straight,
    Premultiplied,
}

/// Code-owned port identity and content interpretation, independent of native shape. The Host
/// and adapter must agree on that interpretation; a native format alone does not select a port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextureMeaning {
    pub(crate) semantic: TypeId,
    pub(crate) transfer: TextureTransfer,
    pub(crate) alpha: TextureAlpha,
}

impl TextureMeaning {
    pub(crate) fn of<T: 'static>(transfer: TextureTransfer, alpha: TextureAlpha) -> Self {
        Self {
            semantic: TypeId::of::<T>(),
            transfer,
            alpha,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferMeaning {
    pub(crate) semantic: TypeId,
}

impl BufferMeaning {
    pub(crate) fn of<T: 'static>() -> Self {
        Self {
            semantic: TypeId::of::<T>(),
        }
    }
}

/// Owned exact native descriptors; static labels and view-format slices stay in static storage.
/// A descriptor is neither a native resource nor evidence of initialized physical custody.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResourceDescriptor {
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

impl ResourceDescriptor {
    fn validate(&self, limits: GraphLimits) -> Result<u64, GraphError> {
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
                // Initially admit baseline, uncompressed color formats only. Optional features,
                // planes and depth/stencil need an explicit scope extension and Host consumer.
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

    /// A copy preserves bytes and color interpretation, but can initialize a different semantic
    /// output port. No reinterpretation, resize, resolve or conversion is implicit here.
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

/// Whole-resource dependency roles, independent of native pipeline and binding descriptors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessRole {
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
pub(crate) struct ResourceRead {
    pub(crate) version: ResourceVersion,
    pub(crate) role: AccessRole,
    pub(crate) expected: ResourceDescriptor,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Initialization {
    /// A separate whole-attachment clear, outside any operation viewport/scissor, with Store.
    Clear(wgpu::Color),
    /// A whole-buffer clear through COPY_DST, independent of allocator contents.
    Zero,
    /// Copy the whole predecessor into distinct storage before the operation runs. The old
    /// version remains immutable and can still be read, exported or retained as history.
    Preserve(ResourceVersion),
}

impl Initialization {
    fn predecessor(&self) -> Option<&ResourceVersion> {
        match self {
            Self::Preserve(version) => Some(version),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResourceWrite {
    pub(crate) version: ResourceVersion,
    pub(crate) role: AccessRole,
    pub(crate) initialization: Initialization,
}

impl ResourceWrite {
    /// The Host uses Store both for the initialization clear and every subsequent attachment
    /// pass on this shared version. Discard is available only to adapter-private scratch.
    pub(crate) fn store_op(&self) -> wgpu::StoreOp {
        wgpu::StoreOp::Store
    }
}

/// The independently enforced allowance exceeded during logical graph admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitKind {
    SubmissionResources,
    Resources,
    Operations,
    Accesses,
    Outputs,
    TextureDimension,
    BufferBytes,
    ResourceBytes,
    MetadataBytes,
}

/// Logical candidate rejection. Native resource, device and submission errors belong to the Host.
/// Resource indices identify declarations within the rejected candidate, not reusable GPU handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    InvalidLimits,
    LimitExceeded(LimitKind),
    AllocationFailed,
    ForeignCandidate,
    UnknownResource(usize),
    UnsupportedTextureScope,
    UnsupportedTextureFormat,
    UnsupportedBufferScope,
    UnsupportedUsage,
    DuplicateOperationKey(u64),
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

/// Finite logical graph allowances. Defaults admit ordinary full-resolution composition while
/// bounding counts and modeled bytes; the Host must also enforce its actual enabled device limits.
#[derive(Clone, Copy, Debug)]
pub struct GraphLimits {
    /// Imported and locally reserved versions together, including unused reservations.
    pub max_resources: usize,
    /// Native operation declarations, independently of their number of accesses.
    pub max_operations: usize,
    /// Explicit reads and writes, plus one implicit copy-source read for every Preserve.
    pub max_accesses: usize,
    /// Distinct versions in the candidate's single publication group.
    pub max_outputs: usize,
    /// The Host must clamp this to its actual device limit before candidate construction.
    pub max_texture_dimension_2d: u32,
    /// The Host must likewise admit actual buffer and binding limits at realization.
    pub max_buffer_bytes: u64,
    /// Sum of full texture texel bytes and buffer lengths, counting imports and local versions.
    /// This is a logical allowance, not native heap usage, retained leases or a GPU reservation.
    pub max_modeled_resource_bytes: u64,
    /// Checked Rust-value capacity envelope including derived compilation scratch. Excludes
    /// caller slices, static descriptor text, allocator rounding/headers and native resources;
    /// it is not an RSS ceiling. A large allowance does not cause eager allocation of that size.
    pub max_modeled_metadata_bytes: usize,
}

impl Default for GraphLimits {
    fn default() -> Self {
        Self {
            max_resources: 256,
            max_operations: 256,
            max_accesses: 1024,
            max_outputs: 64,
            max_texture_dimension_2d: 8192,
            max_buffer_bytes: 256 * 1024 * 1024,
            max_modeled_resource_bytes: 512 * 1024 * 1024,
            max_modeled_metadata_bytes: 64 * 1024 * 1024,
        }
    }
}

impl GraphLimits {
    /// Validate every collection capacity and the complete conservative envelope before any
    /// graph-owned allocation, including the identity Arc. Compile never allocates a dense graph.
    pub fn modeled_metadata_bytes(self) -> Result<usize, GraphError> {
        if self.max_resources == 0
            || self.max_outputs == 0
            || self.max_texture_dimension_2d == 0
            || self.max_buffer_bytes == 0
            || self.max_modeled_resource_bytes == 0
        {
            return Err(GraphError::InvalidLimits);
        }
        let mut total = size_of::<GraphBuilder>()
            + size_of::<CompiledGraph>()
            + 2 * size_of::<usize>() // Arc strong/weak counts; its payload is ().
            + 3 * size_of::<Vec<usize>>() // Indegrees, adjacency heads and sparse edge headers.
            + size_of::<BinaryHeap<compile::ReadyOperation>>();
        for (count, stride) in [
            (self.max_resources, size_of::<GraphResource>()),
            (self.max_operations, size_of::<GraphOperation>()),
            (
                self.max_accesses,
                size_of::<ResourceRead>().max(size_of::<ResourceWrite>()),
            ),
            (self.max_outputs, size_of::<ResourceVersion>()),
            // Serial order (retained), indegrees and adjacency heads (temporary).
            (self.max_operations, size_of::<usize>()),
            (self.max_operations, size_of::<usize>()),
            (self.max_operations, size_of::<usize>()),
            (self.max_operations, size_of::<compile::ReadyOperation>()),
            (self.max_accesses, size_of::<compile::DependencyEdge>()),
        ] {
            // Vec and BinaryHeap allocations must also fit Rust's single-allocation bound.
            let bytes = count
                .checked_mul(stride)
                .filter(|bytes| *bytes <= isize::MAX as usize)
                .ok_or(GraphError::LimitExceeded(LimitKind::MetadataBytes))?;
            total = total
                .checked_add(bytes)
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
pub(crate) struct GraphResource {
    pub(crate) version: ResourceVersion,
    pub(crate) desc: ResourceDescriptor,
    origin: Origin,
}

impl GraphResource {
    pub(crate) fn is_imported(&self) -> bool {
        matches!(self.origin, Origin::Imported)
    }
}

/// Only graph declarations. The Host's exact typed native operation is held separately by key.
#[derive(Debug)]
pub(crate) struct GraphOperation {
    pub(crate) key: u64,
    pub(crate) reads: Vec<ResourceRead>,
    pub(crate) writes: Vec<ResourceWrite>,
}

impl GraphOperation {
    fn dependencies(&self) -> impl Iterator<Item = &ResourceVersion> {
        self.reads.iter().map(|read| &read.version).chain(
            self.writes
                .iter()
                .filter_map(|write| write.initialization.predecessor()),
        )
    }
}

#[derive(Debug)]
pub(crate) struct GraphBuilder {
    owner: Arc<()>,
    limits: GraphLimits,
    resources: Vec<GraphResource>,
    operations: Vec<GraphOperation>,
    outputs: Vec<ResourceVersion>,
    accesses: usize,
    modeled_metadata_bytes: usize,
    modeled_resource_bytes: u64,
}

impl GraphBuilder {
    pub(crate) fn new(limits: GraphLimits) -> Result<Self, GraphError> {
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

    pub(crate) fn reserve_resource(
        &mut self,
        desc: ResourceDescriptor,
    ) -> Result<ResourceVersion, GraphError> {
        self.add_resource(desc, Origin::Local { producer: None })
    }

    /// HOST ONLY: call after validating an exact physical lease against this Host/device epoch,
    /// descriptor, full initialized range, interpretation and permitted usages. The Host retains
    /// that lease separately through candidate/submission use. No caller-provided initialization
    /// boolean or state can authenticate an import; an uninitialized resource cannot use this path.
    pub(crate) fn import_resource(
        &mut self,
        desc: ResourceDescriptor,
    ) -> Result<ResourceVersion, GraphError> {
        self.add_resource(desc, Origin::Imported)
    }

    fn add_resource(
        &mut self,
        desc: ResourceDescriptor,
        origin: Origin,
    ) -> Result<ResourceVersion, GraphError> {
        if self.resources.len() >= self.limits.max_resources {
            return Err(GraphError::LimitExceeded(LimitKind::Resources));
        }
        let bytes = desc.validate(self.limits)?;
        let modeled_resource_bytes = self
            .modeled_resource_bytes
            .checked_add(bytes)
            .filter(|bytes| *bytes <= self.limits.max_modeled_resource_bytes)
            .ok_or(GraphError::LimitExceeded(LimitKind::ResourceBytes))?;
        let version = ResourceVersion {
            owner: Arc::clone(&self.owner),
            index: self.resources.len(),
        };
        self.resources.push(GraphResource {
            version: version.clone(),
            desc,
            origin,
        });
        self.modeled_resource_bytes = modeled_resource_bytes;
        Ok(version)
    }

    fn resource(&self, version: &ResourceVersion) -> Result<&GraphResource, GraphError> {
        resource(&self.owner, &self.resources, version)
    }

    /// Count admission and validation precede copying any caller-owned slices. On rejection,
    /// producers, keys, resource allowances and access allowances are unchanged.
    #[cfg(test)]
    pub(crate) fn add_operation(
        &mut self,
        key: u64,
        reads: &[ResourceRead],
        writes: &[ResourceWrite],
    ) -> Result<(), GraphError> {
        self.add_operation_refs(key, reads.iter(), writes.iter())
    }

    pub(crate) fn add_operation_refs<'a>(
        &mut self,
        key: u64,
        reads: impl ExactSizeIterator<Item = &'a ResourceRead> + Clone,
        writes: impl ExactSizeIterator<Item = &'a ResourceWrite> + Clone,
    ) -> Result<(), GraphError> {
        if self.operations.iter().any(|operation| operation.key == key) {
            return Err(GraphError::DuplicateOperationKey(key));
        }
        if self.operations.len() >= self.limits.max_operations {
            return Err(GraphError::LimitExceeded(LimitKind::Operations));
        }
        let accesses = self
            .accesses
            .checked_add(reads.len())
            .and_then(|count| count.checked_add(writes.len()))
            .filter(|count| *count <= self.limits.max_accesses)
            .ok_or(GraphError::LimitExceeded(LimitKind::Accesses))?;
        let copies = writes
            .clone()
            .filter(|write| write.initialization.predecessor().is_some())
            .count();
        let accesses = accesses
            .checked_add(copies)
            .filter(|count| *count <= self.limits.max_accesses)
            .ok_or(GraphError::LimitExceeded(LimitKind::Accesses))?;

        for read in reads.clone() {
            let actual = &self.resource(&read.version)?.desc;
            validate_access(actual, &read.version, read.role, false)?;
            read.expected.validate(self.limits)?;
            if !actual.satisfies(&read.expected) {
                return Err(GraphError::IncompatibleRead(read.version.index));
            }
            if writes.clone().any(|write| write.version == read.version) {
                return Err(GraphError::ReadWriteAlias(read.version.index));
            }
        }
        for (index, write) in writes.clone().enumerate() {
            let output = self.resource(&write.version)?;
            match output.origin {
                Origin::Imported => return Err(GraphError::ImportedWrite(write.version.index)),
                Origin::Local { producer: Some(_) } => {
                    return Err(GraphError::RepeatedProducer(write.version.index));
                }
                Origin::Local { producer: None } => {}
            }
            if writes
                .clone()
                .take(index)
                .any(|other| other.version == write.version)
            {
                return Err(GraphError::RepeatedProducer(write.version.index));
            }
            validate_access(&output.desc, &write.version, write.role, true)?;
            match &write.initialization {
                Initialization::Clear(color) => {
                    if !matches!(output.desc, ResourceDescriptor::Texture { .. })
                        || ![color.r, color.g, color.b, color.a]
                            .into_iter()
                            .all(f64::is_finite)
                    {
                        return Err(GraphError::InvalidInitialization(write.version.index));
                    }
                    validate_access(&output.desc, &write.version, AccessRole::Attachment, true)?;
                }
                Initialization::Zero => {
                    if !matches!(output.desc, ResourceDescriptor::Buffer { .. }) {
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
                    if writes.clone().any(|other| other.version == *predecessor) {
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
        admitted_reads.extend(reads.cloned());
        admitted_writes.extend(writes.clone().cloned());
        let producer = self.operations.len();
        for write in writes {
            self.resources[write.version.index].origin = Origin::Local {
                producer: Some(producer),
            };
        }
        self.operations.push(GraphOperation {
            key,
            reads: admitted_reads,
            writes: admitted_writes,
        });
        self.accesses = accesses;
        Ok(())
    }

    /// The complete publication intent of this candidate, without publication/history effects.
    /// Replacements validate in full before changing the previous group; an empty group rejects.
    #[cfg(test)]
    pub(crate) fn set_output_group(
        &mut self,
        outputs: &[ResourceVersion],
    ) -> Result<(), GraphError> {
        self.set_output_group_refs(outputs.iter())
    }

    pub(crate) fn set_output_group_refs<'a>(
        &mut self,
        outputs: impl ExactSizeIterator<Item = &'a ResourceVersion> + Clone,
    ) -> Result<(), GraphError> {
        if outputs.len() == 0 {
            return Err(GraphError::MissingOutputGroup);
        }
        if outputs.len() > self.limits.max_outputs {
            return Err(GraphError::LimitExceeded(LimitKind::Outputs));
        }
        for (index, output) in outputs.clone().enumerate() {
            self.resource(output)?;
            if outputs
                .clone()
                .take(index)
                .any(|previous| previous == output)
            {
                return Err(GraphError::DuplicateOutput(output.index));
            }
        }
        self.outputs.clear();
        self.outputs.extend(outputs.cloned());
        Ok(())
    }
}

/// Validated logical execution, not proof of native encoding, submission or output publication.
#[derive(Debug)]
pub(crate) struct CompiledGraph {
    owner: Arc<()>,
    resources: Vec<GraphResource>,
    operations: Vec<GraphOperation>,
    order: Vec<usize>,
    outputs: Vec<ResourceVersion>,
    modeled_metadata_bytes: usize,
    modeled_resource_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct CopyDependency {
    pub(crate) source: ResourceVersion,
    pub(crate) destination: ResourceVersion,
    pub(crate) before: u64,
}

impl CompiledGraph {
    pub(crate) fn resources(&self) -> &[GraphResource] {
        &self.resources
    }

    pub(crate) fn resource(&self, version: &ResourceVersion) -> Result<&GraphResource, GraphError> {
        resource(&self.owner, &self.resources, version)
    }

    /// Deterministic key-ordered topological execution. Execute all write initializations before
    /// the corresponding native payload; an attachment then loads initialized contents and Stores.
    pub(crate) fn operations(&self) -> impl ExactSizeIterator<Item = &GraphOperation> {
        self.order.iter().map(|index| &self.operations[*index])
    }

    pub(crate) fn outputs(&self) -> &[ResourceVersion] {
        &self.outputs
    }

    /// Convenience view of the Preserve prepasses; no allocation or native copy is performed.
    #[cfg(test)]
    pub(crate) fn copy_dependencies(&self) -> impl Iterator<Item = CopyDependency> + '_ {
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

    pub(crate) fn modeled_metadata_bytes(&self) -> usize {
        self.modeled_metadata_bytes
    }

    pub(crate) fn modeled_resource_bytes(&self) -> u64 {
        self.modeled_resource_bytes
    }
}

fn resource<'a>(
    owner: &Arc<()>,
    resources: &'a [GraphResource],
    version: &ResourceVersion,
) -> Result<&'a GraphResource, GraphError> {
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
    desc: &ResourceDescriptor,
    version: &ResourceVersion,
    role: AccessRole,
    write: bool,
) -> Result<(), GraphError> {
    if (write && !role.is_write()) || (!write && !role.is_read()) {
        return Err(GraphError::IllegalAccess(role));
    }
    let valid = match desc {
        ResourceDescriptor::Texture { native, .. } => {
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
        ResourceDescriptor::Buffer { native, .. } => {
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
