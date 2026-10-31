//! Finite captured render programs executed by the existing serialized WGPU Host.

mod feature;
mod native;
mod resources;
#[cfg(test)]
mod tests;

use std::{mem::size_of, sync::Arc};

use nara_render::RenderFrameSourceGeneration;
use thiserror::Error;

use crate::{
    WgpuSubmissionPrefix,
    graph::{
        BufferMeaning, CompiledGraph, GraphBuilder, GraphError, GraphLimits, ResourceDescriptor,
        ResourceVersion,
    },
};

pub use crate::preparation::WgpuFeatureAllocation;
pub(crate) use feature::NativeFeatureStates;
use feature::{CapturedOperation, FeatureInstanceIdentity, OperationReport};
pub use feature::{
    WgpuFeatureDefinition, WgpuFeatureInstance, WgpuNativeResources, WgpuRenderFeature,
};
pub(crate) use native::FeaturePreparationAttempt;
pub(crate) use native::prepare_and_encode;
pub use native::{
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuOperationResources,
};
pub(crate) use resources::NativeResource;
use resources::require_descriptor;
pub(crate) use resources::{HostProgramTexture, ProgramResourceStorage};
pub use resources::{
    WgpuProgramBuffer, WgpuProgramBufferOutput, WgpuProgramOutput, WgpuProgramRead,
    WgpuProgramTexture, WgpuProgramTextureDescriptor, WgpuProgramTextureOutput, WgpuProgramWrite,
};

/// A rejected declaration or native Feature operation. No error publishes part of its group.
#[derive(Debug, Error)]
pub enum WgpuRenderProgramError {
    #[error(transparent)]
    Backend(#[from] crate::WgpuRenderError),
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error("the Feature instance is closed or belongs to another Host or producer")]
    FeatureClosed,
    #[error("render Feature identity space is exhausted")]
    IdentityExhausted,
    #[error("the Feature's original native state is unavailable")]
    FeatureState,
    #[error("the resource has a different native kind")]
    ResourceKind,
    #[error("the resource has a different semantic port type")]
    ResourceSemantic,
    #[error("the operation did not declare access to this resource")]
    UndeclaredResource,
    #[error("the resource was not selected for this program's output group")]
    UnpublishedResource,
    #[error("the imported resource belongs to another Host or a retired device epoch")]
    StaleImport,
    #[error("native render-resource reservations are at capacity")]
    ResourceCapacity,
    #[error(
        "the native resource requires {name}={required}, but the enabled device permits {enabled}"
    )]
    DeviceResourceLimit {
        name: &'static str,
        required: u64,
        enabled: u64,
    },
    #[error("the native Feature rejected its capture: {message}")]
    Feature { message: String },
    #[error("native Feature execution panicked; its original state was retired")]
    FeaturePanicked,
}

impl WgpuRenderProgramError {
    #[must_use]
    pub fn feature(message: impl Into<String>) -> Self {
        Self::Feature {
            message: message.into(),
        }
    }
}

impl From<crate::submission::SubmissionCapacity> for WgpuRenderProgramError {
    fn from(error: crate::submission::SubmissionCapacity) -> Self {
        match error {
            crate::submission::SubmissionCapacity::Limit => {
                GraphError::LimitExceeded(crate::LimitKind::SubmissionResources).into()
            }
            crate::submission::SubmissionCapacity::Allocation => Self::ResourceCapacity,
        }
    }
}

impl From<crate::preparation::NativePreparationError> for WgpuRenderProgramError {
    fn from(error: crate::preparation::NativePreparationError) -> Self {
        match error {
            crate::preparation::NativePreparationError::ResourceCapacity => Self::ResourceCapacity,
            crate::preparation::NativePreparationError::SubmissionCapacity => {
                GraphError::LimitExceeded(crate::LimitKind::SubmissionResources).into()
            }
            crate::preparation::NativePreparationError::Backend(error) => Self::Backend(error),
        }
    }
}

pub(crate) struct ProgramOperation {
    pub(crate) key: u64,
    pub(crate) capture: Option<Arc<dyn CapturedOperation>>,
}

pub(crate) struct ProgramImport {
    pub(crate) version: ResourceVersion,
    pub(crate) storage: Arc<ProgramResourceStorage>,
}

pub(crate) struct ProgramOperationReport {
    pub(crate) key: u64,
    pub(crate) instance: Arc<FeatureInstanceIdentity>,
    pub(crate) report: Arc<dyn OperationReport>,
}

pub(crate) struct CompiledProgram {
    pub(crate) source: RenderFrameSourceGeneration,
    pub(crate) frame_index: u64,
    pub(crate) graph: CompiledGraph,
    pub(crate) operations: Vec<ProgramOperation>,
    pub(crate) imports: Vec<ProgramImport>,
    modeled_metadata_bytes: usize,
    pub(crate) max_modeled_metadata_bytes: usize,
}

/// An immutable CPU program with exact captures and imported versions, without GPU submission
/// authority. Retain this input separately when reconstruction on a later device is required.
#[derive(Clone)]
pub struct WgpuRenderProgram(pub(crate) Arc<CompiledProgram>);

impl std::fmt::Debug for WgpuRenderProgram {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuRenderProgram")
            .field("source", &self.0.source)
            .field("frame_index", &self.0.frame_index)
            .field(
                "graph_metadata_bytes",
                &self.0.graph.modeled_metadata_bytes(),
            )
            .field("graph", &self.0.graph)
            .finish_non_exhaustive()
    }
}

impl WgpuRenderProgram {
    #[must_use]
    pub fn source_generation(&self) -> RenderFrameSourceGeneration {
        self.0.source
    }

    #[must_use]
    pub fn frame_index(&self) -> u64 {
        self.0.frame_index
    }

    /// Checked engine value/capacity envelope, excluding indirect user payloads and native
    /// driver/allocator overhead. It is not a measurement of retained memory.
    #[must_use]
    pub fn modeled_metadata_bytes(&self) -> usize {
        self.0.modeled_metadata_bytes
    }

    #[must_use]
    pub fn modeled_resource_bytes(&self) -> u64 {
        self.0.graph.modeled_resource_bytes()
    }
}

/// Builds one complete publication candidate. Stable operation keys only break ready ties;
/// resource reads and preservation declare the actual dependency edges.
pub struct WgpuRenderProgramBuilder {
    source: RenderFrameSourceGeneration,
    frame_index: u64,
    graph: GraphBuilder,
    operations: Vec<ProgramOperation>,
    imports: Vec<ProgramImport>,
    modeled_metadata_bytes: usize,
    max_modeled_metadata_bytes: usize,
}

impl WgpuRenderProgramBuilder {
    pub fn new(
        source: RenderFrameSourceGeneration,
        frame_index: u64,
        limits: GraphLimits,
    ) -> Result<Self, WgpuRenderProgramError> {
        let modeled_metadata_bytes = Self::required_metadata_bytes(limits)?;
        let mut operations = Vec::new();
        operations
            .try_reserve_exact(limits.max_operations)
            .map_err(|_| GraphError::AllocationFailed)?;
        let mut imports = Vec::new();
        imports
            .try_reserve_exact(limits.max_resources)
            .map_err(|_| GraphError::AllocationFailed)?;
        Ok(Self {
            source,
            frame_index,
            graph: GraphBuilder::new(limits)?,
            operations,
            imports,
            modeled_metadata_bytes,
            max_modeled_metadata_bytes: limits.max_modeled_metadata_bytes,
        })
    }

    /// Conservative capacity envelope for graph/scratch, execution side tables, native resource
    /// wrappers and the bounded state index. Each actual typed operation additionally admits its
    /// exact inline capture/state/prepared/report wrappers before allocating a capture wrapper.
    pub fn required_metadata_bytes(limits: GraphLimits) -> Result<usize, WgpuRenderProgramError> {
        let mut bytes = limits.modeled_metadata_bytes()?;
        limits
            .max_operations
            .checked_mul(3)
            .and_then(|entries| entries.checked_add(limits.max_resources))
            .and_then(|entries| entries.checked_add(2))
            .filter(|entries| *entries <= crate::submission::MAX_SUBMISSION_RESOURCES)
            .ok_or(GraphError::LimitExceeded(
                crate::LimitKind::SubmissionResources,
            ))?;
        for fixed in [
            size_of::<Self>(),
            size_of::<CompiledProgram>(),
            size_of::<WgpuProgramOutputs>(),
            size_of::<native::EncodedProgram>(),
            size_of::<native::FeaturePreparationAttempt>(),
            size_of::<native::NativePreparationCost>(),
            size_of::<HostProgramTexture>(),
            size_of::<crate::output::WgpuImageStorage>() + 2 * size_of::<usize>(),
            2 * size_of::<usize>(),
            crate::submission::WgpuSubmissionReservation::modeled_metadata_bytes::<WgpuRenderProgram>(
            ),
            crate::submission::WgpuSubmissionReservation::modeled_retain_bytes::<WgpuRenderProgram>(
            ),
            if limits.max_operations > 0 {
                feature::state_table_metadata_bytes()
            } else {
                0
            },
        ] {
            bytes = bytes
                .checked_add(fixed)
                .ok_or(GraphError::LimitExceeded(crate::LimitKind::MetadataBytes))?;
        }
        for (count, stride) in [
            (limits.max_operations, size_of::<ProgramOperation>()),
            (limits.max_resources, size_of::<ProgramImport>()),
            (
                limits.max_resources,
                size_of::<Arc<ProgramResourceStorage>>(),
            ),
            (limits.max_operations, size_of::<ProgramOperationReport>()),
            (limits.max_operations, size_of::<u64>()),
            (
                limits.max_outputs,
                size_of::<(ResourceVersion, Arc<ProgramResourceStorage>)>(),
            ),
            (
                limits.max_resources,
                size_of::<crate::output::OutputReservation>(),
            ),
            (
                limits.max_resources,
                size_of::<crate::output::OutputReservation>() + 2 * size_of::<usize>(),
            ),
            (
                limits.max_resources,
                size_of::<ProgramResourceStorage>() + 2 * size_of::<usize>(),
            ),
            (
                limits.max_resources,
                crate::submission::WgpuSubmissionReservation::modeled_retain_bytes::<
                    Arc<ProgramResourceStorage>,
                >(),
            ),
        ] {
            let table = count
                .checked_mul(stride)
                .filter(|bytes| *bytes <= isize::MAX as usize)
                .ok_or(GraphError::LimitExceeded(crate::LimitKind::MetadataBytes))?;
            bytes = bytes
                .checked_add(table)
                .ok_or(GraphError::LimitExceeded(crate::LimitKind::MetadataBytes))?;
        }
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            bytes = bytes
                .checked_add(std::mem::size_of::<crate::stock_quad::StockImagePreflight>())
                .and_then(|bytes| {
                    limits
                        .max_operations
                        .checked_mul(
                            std::mem::size_of::<Arc<[crate::quad::WgpuQuadBatch]>>()
                                + std::mem::size_of::<crate::budget::InstanceReservation>(),
                        )
                        .and_then(|table| bytes.checked_add(table))
                })
                .ok_or(GraphError::LimitExceeded(crate::LimitKind::MetadataBytes))?;
        }
        #[cfg(feature = "sprite-submitter")]
        {
            bytes = limits
                .max_operations
                .checked_mul(crate::sprite::ordered_view_metadata_bytes())
                .and_then(|views| bytes.checked_add(views))
                .ok_or(GraphError::LimitExceeded(crate::LimitKind::MetadataBytes))?;
        }
        if bytes > limits.max_modeled_metadata_bytes {
            return Err(GraphError::LimitExceeded(crate::LimitKind::MetadataBytes).into());
        }
        Ok(bytes)
    }

    pub fn texture<S: 'static>(
        &mut self,
        descriptor: WgpuProgramTextureDescriptor,
    ) -> Result<WgpuProgramTexture<S>, WgpuRenderProgramError> {
        let version = self.graph.reserve_resource(descriptor.resource::<S>())?;
        Ok(WgpuProgramTexture::new(version, descriptor))
    }

    pub fn buffer<S: 'static>(
        &mut self,
        descriptor: wgpu::BufferDescriptor<'static>,
    ) -> Result<WgpuProgramBuffer<S>, WgpuRenderProgramError> {
        let version = self.graph.reserve_resource(ResourceDescriptor::Buffer {
            native: descriptor.clone(),
            meaning: BufferMeaning::of::<S>(),
        })?;
        Ok(WgpuProgramBuffer::new(version, descriptor))
    }

    /// Imports an initialized exact Host-issued output. Current Host/epoch is revalidated at
    /// execution; retaining this CPU declaration cannot make an old epoch current again.
    pub fn import_texture<S: 'static>(
        &mut self,
        output: &WgpuProgramTextureOutput<S>,
    ) -> Result<WgpuProgramTexture<S>, WgpuRenderProgramError> {
        let descriptor = output.descriptor();
        let version = self
            .graph
            .import_resource(output.storage.descriptor.clone())?;
        self.imports.push(ProgramImport {
            version: version.clone(),
            storage: Arc::clone(&output.storage),
        });
        Ok(WgpuProgramTexture::new(version, descriptor))
    }

    pub fn import_buffer<S: 'static>(
        &mut self,
        output: &WgpuProgramBufferOutput<S>,
    ) -> Result<WgpuProgramBuffer<S>, WgpuRenderProgramError> {
        let descriptor = output.descriptor().clone();
        let version = self
            .graph
            .import_resource(output.storage.descriptor.clone())?;
        self.imports.push(ProgramImport {
            version: version.clone(),
            storage: Arc::clone(&output.storage),
        });
        Ok(WgpuProgramBuffer::new(version, descriptor))
    }

    /// Initialization-only operation; useful for blank targets and explicit copies. It obeys
    /// the same dependency, native validation, output and submission rules as Feature work.
    pub fn initialize(
        &mut self,
        key: u64,
        writes: &[WgpuProgramWrite],
    ) -> Result<(), WgpuRenderProgramError> {
        self.add(key, None, &[], writes)
    }

    pub fn operation<F: WgpuRenderFeature>(
        &mut self,
        key: u64,
        instance: &WgpuFeatureInstance<F>,
        capture: Arc<F::Capture>,
        reads: &[WgpuProgramRead],
        writes: &[WgpuProgramWrite],
    ) -> Result<(), WgpuRenderProgramError> {
        if instance.identity.is_closed() {
            return Err(WgpuRenderProgramError::FeatureClosed);
        }
        let metadata = feature::operation_metadata_bytes::<F>()
            .and_then(|bytes| self.modeled_metadata_bytes.checked_add(bytes))
            .filter(|bytes| *bytes <= self.max_modeled_metadata_bytes)
            .ok_or(GraphError::LimitExceeded(crate::LimitKind::MetadataBytes))?;
        self.graph.add_operation_refs(
            key,
            reads.iter().map(|read| &read.0),
            writes.iter().map(|write| &write.0),
        )?;
        // Declaration validation and count/metadata admission precede this engine-owned Arc.
        // A concurrent close after admission is revalidated by the actual Host before use.
        self.operations.push(ProgramOperation {
            key,
            capture: Some(instance.capture(capture)),
        });
        self.modeled_metadata_bytes = metadata;
        Ok(())
    }

    fn add(
        &mut self,
        key: u64,
        capture: Option<Arc<dyn CapturedOperation>>,
        reads: &[WgpuProgramRead],
        writes: &[WgpuProgramWrite],
    ) -> Result<(), WgpuRenderProgramError> {
        self.graph.add_operation_refs(
            key,
            reads.iter().map(|read| &read.0),
            writes.iter().map(|write| &write.0),
        )?;
        self.operations.push(ProgramOperation { key, capture });
        Ok(())
    }

    pub fn finish(
        mut self,
        outputs: &[WgpuProgramOutput],
    ) -> Result<WgpuRenderProgram, WgpuRenderProgramError> {
        self.graph
            .set_output_group_refs(outputs.iter().map(|output| &output.0))?;
        let graph = self.graph.compile()?;
        Ok(WgpuRenderProgram(Arc::new(CompiledProgram {
            source: self.source,
            frame_index: self.frame_index,
            graph,
            operations: self.operations,
            imports: self.imports,
            modeled_metadata_bytes: self.modeled_metadata_bytes,
            max_modeled_metadata_bytes: self.max_modeled_metadata_bytes,
        })))
    }
}

/// A successfully admitted complete group. Resource lookup uses the original candidate slot;
/// vector position, operation registration order and a later registry cannot retarget it.
pub struct WgpuProgramOutputs {
    pub(crate) outputs: Vec<(ResourceVersion, Arc<ProgramResourceStorage>)>,
    pub(crate) reports: Vec<ProgramOperationReport>,
    pub(crate) prefix: WgpuSubmissionPrefix,
}

impl std::fmt::Debug for WgpuProgramOutputs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuProgramOutputs")
            .field("output_count", &self.outputs.len())
            .field("report_count", &self.reports.len())
            .finish_non_exhaustive()
    }
}

impl WgpuProgramOutputs {
    fn storage(
        &self,
        version: &ResourceVersion,
    ) -> Result<Arc<ProgramResourceStorage>, WgpuRenderProgramError> {
        self.outputs
            .iter()
            .find(|(slot, _)| slot == version)
            .map(|(_, storage)| Arc::clone(storage))
            .ok_or(WgpuRenderProgramError::UnpublishedResource)
    }

    pub fn texture<S: 'static>(
        &self,
        port: &WgpuProgramTexture<S>,
    ) -> Result<WgpuProgramTextureOutput<S>, WgpuRenderProgramError> {
        let storage = self.storage(&port.version)?;
        require_descriptor::<S>(&storage.descriptor, true)?;
        Ok(WgpuProgramTextureOutput::new(storage))
    }

    pub fn buffer<S: 'static>(
        &self,
        port: &WgpuProgramBuffer<S>,
    ) -> Result<WgpuProgramBufferOutput<S>, WgpuRenderProgramError> {
        let storage = self.storage(&port.version)?;
        require_descriptor::<S>(&storage.descriptor, false)?;
        Ok(WgpuProgramBufferOutput::new(storage))
    }

    /// Returns the original adapter's typed report only after the whole group was admitted.
    /// A later instance, even of the same Rust type, cannot read this operation's report.
    pub fn report<F: WgpuRenderFeature>(
        &self,
        key: u64,
        instance: &WgpuFeatureInstance<F>,
    ) -> Result<&F::Report, WgpuRenderProgramError> {
        let report = self
            .reports
            .iter()
            .find(|report| report.key == key && Arc::ptr_eq(&report.instance, &instance.identity))
            .ok_or(WgpuRenderProgramError::FeatureState)?;
        report
            .report
            .as_any()
            .downcast_ref::<F::Report>()
            .ok_or(WgpuRenderProgramError::FeatureState)
    }

    /// This original prefix controls physical custody, independently of publication success.
    pub fn submitted_prefix(&self) -> &WgpuSubmissionPrefix {
        &self.prefix
    }
}
