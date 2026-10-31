//! Private exact-wgpu executor for the disposable graph comparison.
//!
//! Callers retain trusted native authority. The callback can capture a Device, but this module
//! supplies no Queue or target finalization to it. This is neither a sandbox nor qualification
//! of the production Host, device epochs, streams, output publication or history transitions.
//! Imported initialization, semantic meaning and external immutability remain caller assertions.
//! Native shape/usage checks cannot authenticate an import's device identity or detect destroy().
//!
//! Submission happens only after complete encoding and successful device error-scope inspection.
//! A returned submission retains its original graph and resource table. Separately, its completion
//! callback retains cloned physical resources until the accepted queue prefix completes, including
//! when the returned value is dropped. Callers drive progress and may poll the original index;
//! this executor never waits for an empty queue or uses completion as output-publication authority.
//! The fixture's queue owner serializes submit and callback registration with other submissions,
//! so the callback observes this exact prefix. Cross-thread Host submission is separate work.

use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use super::{
    gpu::Gpu,
    graph::{
        CompiledGraph, GraphError, Initialization, Operation, OperationKey, ResourceDesc, Version,
    },
};

const MAX_RESOURCES: usize = 128;
const MAX_OPERATIONS: usize = 128;
const MAX_ACCESSES: usize = 512;

#[derive(Clone, Debug)]
pub(super) enum NativeResource {
    Texture(Arc<wgpu::Texture>),
    Buffer(Arc<wgpu::Buffer>),
}

#[derive(Debug)]
pub(super) enum NativeError {
    Graph(GraphError),
    LimitExceeded(&'static str),
    AllocationFailed,
    UnexpectedImport(usize),
    DuplicateImport(usize),
    MissingImport(usize),
    ImportKind(usize),
    ImportShape(usize),
    ImportUsage(usize),
    ResourceKind {
        version: usize,
        expected: &'static str,
    },
    /// A concrete kernel can reject admission without allocating an owned error string.
    Callback(&'static str),
    CallbackPanicked {
        operation: OperationKey,
    },
    NativePanicked {
        stage: &'static str,
    },
    /// Preserve all matching scope results even when the callback also returned an error.
    DeviceErrors {
        stage: &'static str,
        validation: Option<wgpu::Error>,
        out_of_memory: Option<wgpu::Error>,
        internal: Option<wgpu::Error>,
    },
}

impl From<GraphError> for NativeError {
    fn from(error: GraphError) -> Self {
        Self::Graph(error)
    }
}

impl fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for NativeError {}

/// Own the original graph so every lookup checks its nonreused candidate identity before indexing.
/// The physical table has exactly one entry per graph version and is immutable after allocation.
#[derive(Debug)]
pub(super) struct NativeResources {
    graph: CompiledGraph,
    values: Vec<NativeResource>,
}

impl NativeResources {
    pub(super) fn graph(&self) -> &CompiledGraph {
        &self.graph
    }

    pub(super) fn get(&self, version: &Version) -> Result<&NativeResource, NativeError> {
        self.graph.resource(version)?;
        // The table and graph are built together after complete import admission.
        Ok(&self.values[version.index()])
    }

    pub(super) fn texture(&self, version: &Version) -> Result<&wgpu::Texture, NativeError> {
        match self.get(version)? {
            NativeResource::Texture(texture) => Ok(texture.as_ref()),
            NativeResource::Buffer(_) => Err(NativeError::ResourceKind {
                version: version.index(),
                expected: "texture",
            }),
        }
    }

    pub(super) fn buffer(&self, version: &Version) -> Result<&wgpu::Buffer, NativeError> {
        match self.get(version)? {
            NativeResource::Buffer(buffer) => Ok(buffer.as_ref()),
            NativeResource::Texture(_) => Err(NativeError::ResourceKind {
                version: version.index(),
                expected: "buffer",
            }),
        }
    }
}

/// Read-only access to the original prefix-completion flag, independent of result custody.
#[derive(Clone, Debug)]
pub(super) struct CompletionObservation(Arc<AtomicBool>);

impl CompletionObservation {
    pub(super) fn is_complete(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
pub(super) struct SubmittedGraph {
    resources: NativeResources,
    submission: wgpu::SubmissionIndex,
    complete: Arc<AtomicBool>,
}

/// Fully encoded unpublished work. Dropping it submits nothing; physical output/history
/// publication remains an explicit caller operation after `submit` succeeds.
#[derive(Debug)]
pub(super) struct ReadyGraph {
    resources: NativeResources,
    commands: wgpu::CommandBuffer,
    retained: Vec<NativeResource>,
}

impl ReadyGraph {
    pub(super) fn resources(&self) -> &NativeResources {
        &self.resources
    }
}

impl SubmittedGraph {
    pub(super) fn graph(&self) -> &CompiledGraph {
        self.resources.graph()
    }

    pub(super) fn resources(&self) -> &NativeResources {
        &self.resources
    }

    /// Clone this exact index when calling Device::poll for this submission prefix.
    pub(super) fn submission_index(&self) -> &wgpu::SubmissionIndex {
        &self.submission
    }

    pub(super) fn is_complete(&self) -> bool {
        self.complete.load(Ordering::Acquire)
    }

    /// Observe the original callback after dropping this result without acquiring write access.
    pub(super) fn completion_observation(&self) -> CompletionObservation {
        CompletionObservation(Arc::clone(&self.complete))
    }
}

pub(super) fn execute(
    gpu: &Gpu,
    graph: CompiledGraph,
    imports: &[(Version, NativeResource)],
    encode: impl FnMut(
        &Operation,
        &NativeResources,
        &mut wgpu::CommandEncoder,
    ) -> Result<(), NativeError>,
) -> Result<SubmittedGraph, NativeError> {
    submit(gpu, prepare(gpu, graph, imports, encode)?)
}

pub(super) fn prepare(
    gpu: &Gpu,
    graph: CompiledGraph,
    imports: &[(Version, NativeResource)],
    mut encode: impl FnMut(
        &Operation,
        &NativeResources,
        &mut wgpu::CommandEncoder,
    ) -> Result<(), NativeError>,
) -> Result<ReadyGraph, NativeError> {
    validate_counts(&graph)?;
    let import_positions = validate_imports(&graph, imports)?;
    let count = graph.resources().len();
    // Preallocate both graph-owned tables before creating any local physical resource. Their
    // modeled payload is 2 * count * size_of::<NativeResource>(), plus fixed stack/Arc values;
    // allocator bookkeeping, driver resources and caller-owned imports are not RSS claims.
    let mut values = reserved(count)?;
    let mut retained = reserved(count)?;

    scoped(&gpu.device, "resource creation", || {
        for resource in graph.resources() {
            let native = if let Some(position) = import_positions[resource.version.index()] {
                imports[position].1.clone()
            } else {
                // Each local version creates a separate native object; Preserve never reuses
                // its predecessor's object, even when the descriptors happen to be identical.
                match &resource.desc {
                    ResourceDesc::Texture { native, .. } => {
                        NativeResource::Texture(Arc::new(gpu.device.create_texture(native)))
                    }
                    ResourceDesc::Buffer { native, .. } => {
                        NativeResource::Buffer(Arc::new(gpu.device.create_buffer(native)))
                    }
                }
            };
            values.push(native);
        }
        Ok(())
    })?;
    retained.extend(values.iter().cloned());
    let resources = NativeResources { graph, values };

    let commands = scoped(&gpu.device, "candidate encoding", || {
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-comparison-graph"),
            });
        for operation in resources.graph().operations() {
            initialize(operation, &resources, &mut encoder)?;
            // AssertUnwindSafe protects only this disposable candidate's failure boundary.
            // Callback-owned state may have changed; it is never retried here after a panic.
            catch_unwind(AssertUnwindSafe(|| {
                encode(operation, &resources, &mut encoder)
            }))
            .map_err(|_| NativeError::CallbackPanicked {
                operation: operation.key,
            })??;
        }
        Ok(encoder.finish())
    })?;

    Ok(ReadyGraph {
        resources,
        commands,
        retained,
    })
}

pub(super) fn submit(gpu: &Gpu, ready: ReadyGraph) -> Result<SubmittedGraph, NativeError> {
    let ReadyGraph {
        resources,
        commands,
        retained,
    } = ready;
    let complete = Arc::new(AtomicBool::new(false));
    let prefix_complete = Arc::clone(&complete);

    // Encoding errors, callback errors and callback panics have all returned before this point.
    // Submission itself can still fail (for example, an externally destroyed imported object).
    // Inspect those scopes too and never return a successful SubmittedGraph on such failure.
    let submission = scoped(&gpu.device, "queue submission", || {
        let result = catch_unwind(AssertUnwindSafe(|| gpu.queue.submit([commands])));
        // Register immediately, before inspecting scopes or returning to the caller. Even if
        // submit panicked, conservatively retain through the queue's last accepted prefix.
        // This closure owns the clones, not the returned SubmittedGraph or a temporary guard.
        gpu.queue.on_submitted_work_done(move || {
            drop(retained);
            prefix_complete.store(true, Ordering::Release);
        });
        result.map_err(|_| NativeError::NativePanicked {
            stage: "queue submission",
        })
    })?;

    Ok(SubmittedGraph {
        resources,
        submission,
        complete,
    })
}

fn validate_counts(graph: &CompiledGraph) -> Result<(), NativeError> {
    if graph.resources().len() > MAX_RESOURCES {
        return Err(NativeError::LimitExceeded("resources"));
    }
    if graph.operations().len() > MAX_OPERATIONS {
        return Err(NativeError::LimitExceeded("operations"));
    }
    let mut accesses = 0usize;
    for operation in graph.operations() {
        accesses = accesses
            .checked_add(operation.reads.len())
            .and_then(|count| count.checked_add(operation.writes.len()))
            .filter(|count| *count <= MAX_ACCESSES)
            .ok_or(NativeError::LimitExceeded("accesses"))?;
        let copies = operation
            .writes
            .iter()
            .filter(|write| matches!(&write.initialization, Initialization::Preserve(_)))
            .count();
        accesses = accesses
            .checked_add(copies)
            .filter(|count| *count <= MAX_ACCESSES)
            .ok_or(NativeError::LimitExceeded("accesses"))?;
    }
    Ok(())
}

/// No local physical allocation, encoder, or kernel callback occurs before this completes.
fn validate_imports(
    graph: &CompiledGraph,
    imports: &[(Version, NativeResource)],
) -> Result<[Option<usize>; MAX_RESOURCES], NativeError> {
    if imports.len() > graph.resources().len() {
        return Err(NativeError::LimitExceeded("imports"));
    }
    let mut positions = [None; MAX_RESOURCES];
    for (position, (version, native)) in imports.iter().enumerate() {
        let resource = graph.resource(version)?;
        if !resource.is_imported() {
            return Err(NativeError::UnexpectedImport(version.index()));
        }
        if positions[version.index()].is_some() {
            return Err(NativeError::DuplicateImport(version.index()));
        }
        validate_import(version, &resource.desc, native)?;
        positions[version.index()] = Some(position);
    }
    for resource in graph.resources() {
        if resource.is_imported() && positions[resource.version.index()].is_none() {
            return Err(NativeError::MissingImport(resource.version.index()));
        }
    }
    Ok(positions)
}

fn validate_import(
    version: &Version,
    desc: &ResourceDesc,
    native: &NativeResource,
) -> Result<(), NativeError> {
    let index = version.index();
    match (desc, native) {
        (
            ResourceDesc::Texture {
                native: expected, ..
            },
            NativeResource::Texture(actual),
        ) => {
            if actual.size() != expected.size
                || actual.dimension() != expected.dimension
                || actual.mip_level_count() != expected.mip_level_count
                || actual.sample_count() != expected.sample_count
                || actual.format() != expected.format
            {
                return Err(NativeError::ImportShape(index));
            }
            if !actual.usage().contains(expected.usage) {
                return Err(NativeError::ImportUsage(index));
            }
        }
        (
            ResourceDesc::Buffer {
                native: expected, ..
            },
            NativeResource::Buffer(actual),
        ) => {
            if actual.size() != expected.size {
                return Err(NativeError::ImportShape(index));
            }
            if !actual.usage().contains(expected.usage) {
                return Err(NativeError::ImportUsage(index));
            }
        }
        _ => return Err(NativeError::ImportKind(index)),
    }
    Ok(())
}

fn initialize(
    operation: &Operation,
    resources: &NativeResources,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), NativeError> {
    // Clear/zero/copy every output before invoking the kernel, including storage ReadWrite.
    // The graph has already rejected same-operation preservation and read/write feedback.
    for write in &operation.writes {
        match &write.initialization {
            Initialization::Clear(color) => {
                let texture = resources.texture(&write.version)?;
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let attachments = [Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(*color),
                        store: wgpu::StoreOp::Store,
                    },
                })];
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("render-comparison-initialize-texture"),
                    color_attachments: &attachments,
                    ..Default::default()
                });
                drop(pass);
            }
            Initialization::Zero => {
                encoder.clear_buffer(resources.buffer(&write.version)?, 0, None);
            }
            Initialization::Preserve(predecessor) => {
                match (resources.get(predecessor)?, resources.get(&write.version)?) {
                    (NativeResource::Texture(source), NativeResource::Texture(destination)) => {
                        encoder.copy_texture_to_texture(
                            source.as_image_copy(),
                            destination.as_image_copy(),
                            source.size(),
                        );
                    }
                    (NativeResource::Buffer(source), NativeResource::Buffer(destination)) => {
                        encoder.copy_buffer_to_buffer(
                            source.as_ref(),
                            0,
                            destination.as_ref(),
                            0,
                            source.size(),
                        );
                    }
                    _ => {
                        return Err(NativeError::ResourceKind {
                            version: write.version.index(),
                            expected: "predecessor kind",
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// Each stage drains all three native scopes in reverse order, including on callback failure.
/// Error-scope futures inspect errors; no GPU completion poll or queue-empty wait is performed.
fn scoped<T>(
    device: &wgpu::Device,
    stage: &'static str,
    work: impl FnOnce() -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    let internal_scope = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let oom_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = catch_unwind(AssertUnwindSafe(work));
    let validation = pollster::block_on(validation_scope.pop());
    let out_of_memory = pollster::block_on(oom_scope.pop());
    let internal = pollster::block_on(internal_scope.pop());
    if validation.is_some() || out_of_memory.is_some() || internal.is_some() {
        return Err(NativeError::DeviceErrors {
            stage,
            validation,
            out_of_memory,
            internal,
        });
    }
    result.map_err(|_| NativeError::NativePanicked { stage })?
}

fn reserved<T>(count: usize) -> Result<Vec<T>, NativeError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| NativeError::AllocationFailed)?;
    Ok(values)
}
