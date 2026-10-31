//! Native realization inside the existing Host's original submission reservation.

use std::{mem::size_of, sync::Arc};

use crate::{
    WgpuRenderProducerId,
    backend::WgpuDeviceNamespace,
    graph::{
        CompiledGraph, GraphError, GraphOperation, Initialization, ResourceDescriptor,
        ResourceVersion,
    },
    output::{OutputBudget, OutputReservation},
    submission::WgpuSubmissionReservation,
};

use super::{
    NativeFeatureStates, ProgramOperationReport, WgpuNativeResources, WgpuProgramBuffer,
    WgpuProgramTexture, WgpuRenderProgram, WgpuRenderProgramError,
    resources::{NativeResource, ProgramResourceStorage, require_descriptor},
};

/// The exact native resources declared by one operation in one compiled program.
///
/// This copyable view borrows the original graph, operation and physical resource table. A
/// lookup checks candidate ownership and declared access before indexing that table. Native
/// adapters must respect the declared roles and immutable imports even when cloning a handle.
#[derive(Clone, Copy)]
pub struct WgpuOperationResources<'a> {
    graph: &'a CompiledGraph,
    operation: &'a GraphOperation,
    physical: &'a [Arc<ProgramResourceStorage>],
}

impl<'a> WgpuOperationResources<'a> {
    /// Borrows the exact whole-texture view, including its semantic port interpretation.
    pub fn texture<S: 'static>(
        &self,
        port: &WgpuProgramTexture<S>,
    ) -> Result<&'a wgpu::TextureView, WgpuRenderProgramError> {
        let storage = self.texture_storage(port)?;
        match &storage.native {
            NativeResource::Texture { view, .. } => Ok(view),
            NativeResource::Buffer(_) => Err(WgpuRenderProgramError::ResourceKind),
        }
    }

    /// Borrows the exact native texture for an explicitly declared copy or storage operation.
    pub fn texture_native<S: 'static>(
        &self,
        port: &WgpuProgramTexture<S>,
    ) -> Result<&'a wgpu::Texture, WgpuRenderProgramError> {
        let storage = self.texture_storage(port)?;
        match &storage.native {
            NativeResource::Texture { texture, .. } => Ok(texture),
            NativeResource::Buffer(_) => Err(WgpuRenderProgramError::ResourceKind),
        }
    }

    /// Borrows the exact whole buffer with the original semantic port type.
    pub fn buffer<S: 'static>(
        &self,
        port: &WgpuProgramBuffer<S>,
    ) -> Result<&'a wgpu::Buffer, WgpuRenderProgramError> {
        let storage = self.storage(&port.version)?;
        require_descriptor::<S>(&storage.descriptor, false)?;
        if storage.descriptor != port.resource() {
            return Err(GraphError::IncompatibleRead(port.version.index()).into());
        }
        match &storage.native {
            NativeResource::Buffer(buffer) => Ok(buffer),
            NativeResource::Texture { .. } => Err(WgpuRenderProgramError::ResourceKind),
        }
    }

    fn texture_storage<S: 'static>(
        &self,
        port: &WgpuProgramTexture<S>,
    ) -> Result<&'a ProgramResourceStorage, WgpuRenderProgramError> {
        let storage = self.storage(&port.version)?;
        require_descriptor::<S>(&storage.descriptor, true)?;
        if storage.descriptor != port.descriptor.resource::<S>() {
            return Err(GraphError::IncompatibleRead(port.version.index()).into());
        }
        Ok(storage)
    }

    fn storage(
        &self,
        version: &ResourceVersion,
    ) -> Result<&'a ProgramResourceStorage, WgpuRenderProgramError> {
        let logical = self.graph.resource(version)?;
        let declared = self.operation.reads.iter().any(|read| read.version == *version)
            || self.operation.writes.iter().any(|write| {
                write.version == *version
                    || matches!(&write.initialization, Initialization::Preserve(source) if source == version)
            });
        if !declared {
            return Err(WgpuRenderProgramError::UndeclaredResource);
        }
        let storage = self
            .physical
            .get(version.index())
            .ok_or(GraphError::UnknownResource(version.index()))?;
        if storage.descriptor != logical.desc {
            return Err(GraphError::IncompatibleRead(version.index()).into());
        }
        Ok(storage)
    }
}

/// Shared retained-byte accounting for an adapter-private native allocation.
///
/// Reserve before allocating and keep a clone alongside the actual allocation in Feature state
/// or prepared work. Clones share one charge. The original submission also retains the guard,
/// including when preparation fails after an upload.
#[derive(Clone, Debug)]
pub struct WgpuFeatureAllocation {
    _reservation: Arc<OutputReservation>,
}

/// Native preparation under the original Host's pre-upload resource custody.
///
/// Queue uploads become visible before the submitted command buffer executes. Use them for
/// adapter-private inputs; shared graph writes still execute their declared initialization before
/// Feature encoding. The Host observes native validation and owns the submission attempt.
pub struct WgpuFeaturePrepareContext<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    resources: WgpuOperationResources<'a>,
    budget: &'a Arc<OutputBudget>,
    reservation: &'a mut WgpuSubmissionReservation,
}

impl<'a> WgpuFeaturePrepareContext<'a> {
    #[must_use]
    pub fn device(&self) -> &'a wgpu::Device {
        self.device
    }

    /// The returned view borrows the original resources independently of this context borrow.
    #[must_use]
    pub fn resources(&self) -> WgpuOperationResources<'a> {
        self.resources
    }

    /// Retains the actual destination handle before the original queue can accept the upload.
    pub fn upload_buffer(
        &mut self,
        destination: &wgpu::Buffer,
        offset: u64,
        data: &[u8],
    ) -> Result<(), WgpuRenderProgramError> {
        self.retain(destination.clone())?;
        self.queue.write_buffer(destination, offset, data);
        Ok(())
    }

    /// Retains the actual destination texture before the original queue can accept the upload.
    pub fn upload_texture(
        &mut self,
        destination: wgpu::TexelCopyTextureInfo<'_>,
        data: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) -> Result<(), WgpuRenderProgramError> {
        self.retain(destination.texture.clone())?;
        self.queue.write_texture(destination, data, layout, size);
        Ok(())
    }

    /// Keeps native resources and their accounting alive through this original submission use.
    pub fn retain<T: WgpuNativeResources>(
        &mut self,
        value: T,
    ) -> Result<(), WgpuRenderProgramError> {
        self.reservation
            .retain(value)
            .map_err(|(error, _)| WgpuRenderProgramError::from(error))
    }

    /// Reserves from the same retained-output budget as local graph resources.
    pub fn reserve_bytes(
        &mut self,
        bytes: u64,
    ) -> Result<WgpuFeatureAllocation, WgpuRenderProgramError> {
        let allocation = WgpuFeatureAllocation {
            _reservation: Arc::new(
                self.budget
                    .reserve(bytes)
                    .ok_or(WgpuRenderProgramError::ResourceCapacity)?,
            ),
        };
        self.retain(allocation.clone())?;
        Ok(allocation)
    }
}

/// Encodes one Feature against already initialized shared resources.
///
/// Every shared attachment pass must load the initialized contents and use `StoreOp::Store`.
/// Adapter-private scratch may use its own load/store policy. Resource references have the
/// original table's lifetime, so they can coexist with a mutable encoder borrow.
pub struct WgpuFeatureEncodeContext<'a> {
    resources: WgpuOperationResources<'a>,
    encoder: &'a mut wgpu::CommandEncoder,
}

impl<'a> WgpuFeatureEncodeContext<'a> {
    #[must_use]
    pub fn resources(&self) -> WgpuOperationResources<'a> {
        self.resources
    }

    pub fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        self.encoder
    }

    /// Borrows both fields for code that constructs pass attachments and encodes in one scope.
    pub fn split(&mut self) -> (WgpuOperationResources<'a>, &mut wgpu::CommandEncoder) {
        (self.resources, self.encoder)
    }
}

/// Encoded work only. Native scopes, submission and complete output publication belong to Host.
pub(crate) struct EncodedProgram {
    pub(crate) commands: wgpu::CommandBuffer,
    /// Exact `ResourceVersion::index` order; the Host selects only the published output leases.
    pub(crate) resources: Vec<Arc<ProgramResourceStorage>>,
    pub(crate) reports: Vec<ProgramOperationReport>,
}

pub(crate) fn prepare_and_encode(
    program: &WgpuRenderProgram,
    namespace: &WgpuDeviceNamespace,
    producer: WgpuRenderProducerId,
    states: &mut NativeFeatureStates,
    budget: &Arc<OutputBudget>,
    reservation: &mut WgpuSubmissionReservation,
) -> Result<EncodedProgram, WgpuRenderProgramError> {
    let compiled = &program.0;
    let graph = &compiled.graph;
    // Admit both retained result capacities before any native allocation or adapter upload.
    let mut resources = reserved::<Arc<ProgramResourceStorage>>(graph.resources().len())?;
    let mut reports = reserved::<ProgramOperationReport>(compiled.operations.len())?;
    validate_imports(program, namespace)?;
    for resource in graph.resources() {
        graph.resource(&resource.version)?;
        validate_device_size(&resource.desc, &namespace.device.limits())?;
        resource_bytes(&resource.desc)?;
    }
    for operation in graph.operations() {
        let original = compiled
            .operations
            .iter()
            .find(|entry| entry.key == operation.key)
            .ok_or(WgpuRenderProgramError::FeatureState)?;
        if let Some(capture) = &original.capture {
            let identity = capture.identity();
            if identity.is_closed()
                || identity.backend_instance_id != namespace.backend_instance_id
                || identity.producer != producer
            {
                return Err(WgpuRenderProgramError::FeatureClosed);
            }
        }
    }

    // Refuse the complete known local working set before any native allocation or prefix
    // retention. On refusal the temporary table drops every earlier guard immediately.
    let mut allocations = reserve_local_allocations(graph, budget)?.into_iter();

    // Keep original captures/definitions and all imports even if state creation or preparation
    // fails. This custody ends with the prefix and is never transferred into published outputs.
    reservation
        .retain(program.clone())
        .map_err(|(error, _)| WgpuRenderProgramError::from(error))?;
    for resource in graph.resources() {
        graph.resource(&resource.version)?;
        if resource.version.index() != resources.len() {
            return Err(GraphError::UnknownResource(resource.version.index()).into());
        }
        let storage = if resource.is_imported() {
            let import = compiled
                .imports
                .iter()
                .find(|entry| entry.version == resource.version)
                .ok_or(WgpuRenderProgramError::StaleImport)?;
            Arc::clone(&import.storage)
        } else {
            // Every local version is a distinct physical allocation and a distinct charge,
            // including preservation destinations and resources not selected as outputs.
            let allocation = allocations
                .next()
                .expect("every local resource has a preflight reservation");
            let native = match &resource.desc {
                ResourceDescriptor::Texture { native, .. } => {
                    let texture = namespace.device.create_texture(native);
                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    NativeResource::Texture { texture, view }
                }
                ResourceDescriptor::Buffer { native, .. } => {
                    NativeResource::Buffer(namespace.device.create_buffer(native))
                }
            };
            Arc::new(ProgramResourceStorage {
                backend_instance_id: namespace.backend_instance_id,
                device_epoch: namespace.device_epoch,
                epoch_lost: Arc::clone(&namespace.epoch_lost),
                producer,
                source: compiled.source,
                frame_index: compiled.frame_index,
                descriptor: resource.desc.clone(),
                native,
                _reservation: allocation,
            })
        };
        // Never rely on the success return to establish physical custody. A later upload,
        // Feature error or encoder panic must retain every resource already realized here.
        reservation
            .retain(Arc::clone(&storage))
            .map_err(|(error, _)| WgpuRenderProgramError::from(error))?;
        resources.push(storage);
    }

    let mut encoder = namespace
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("nara_render_program"),
        });
    for operation in graph.operations() {
        let scoped = WgpuOperationResources {
            graph,
            operation,
            physical: &resources,
        };
        initialize(scoped, &mut encoder)?;
        let original = compiled
            .operations
            .iter()
            .find(|entry| entry.key == operation.key)
            .ok_or(WgpuRenderProgramError::FeatureState)?;
        let Some(capture) = &original.capture else {
            continue;
        };
        let prepared = capture.prepare(
            states,
            namespace.device_epoch,
            &mut WgpuFeaturePrepareContext {
                device: &namespace.device,
                queue: &namespace.queue,
                resources: scoped,
                budget,
                reservation,
            },
        )?;
        reservation
            .retain(Arc::clone(&prepared))
            .map_err(|(error, _)| WgpuRenderProgramError::from(error))?;
        let report = prepared.encode(&mut WgpuFeatureEncodeContext {
            resources: scoped,
            encoder: &mut encoder,
        })?;
        reservation
            .retain(Arc::clone(&report))
            .map_err(|(error, _)| WgpuRenderProgramError::from(error))?;
        reports.push(ProgramOperationReport {
            key: operation.key,
            instance: Arc::clone(capture.identity()),
            report,
        });
    }
    Ok(EncodedProgram {
        commands: encoder.finish(),
        resources,
        reports,
    })
}

fn reserve_local_allocations(
    graph: &CompiledGraph,
    budget: &Arc<OutputBudget>,
) -> Result<Vec<OutputReservation>, WgpuRenderProgramError> {
    let locals = graph
        .resources()
        .iter()
        .filter(|resource| !resource.is_imported());
    // This table's worst-case capacity is included in the program metadata envelope. Admit
    // its actual capacity before acquiring guards so no growth can strand a partial admission.
    let mut allocations = reserved::<OutputReservation>(locals.clone().count())?;
    for resource in locals {
        allocations.push(
            budget
                .reserve(resource_bytes(&resource.desc)?)
                .ok_or(WgpuRenderProgramError::ResourceCapacity)?,
        );
    }
    Ok(allocations)
}

fn validate_imports(
    program: &WgpuRenderProgram,
    namespace: &WgpuDeviceNamespace,
) -> Result<(), WgpuRenderProgramError> {
    let graph = &program.0.graph;
    for (index, import) in program.0.imports.iter().enumerate() {
        let logical = graph.resource(&import.version)?;
        let storage = &import.storage;
        if !logical.is_imported()
            || program.0.imports[..index]
                .iter()
                .any(|other| other.version == import.version)
            || storage.backend_instance_id != namespace.backend_instance_id
            || storage.device_epoch != namespace.device_epoch
            || !storage.is_current()
            || namespace
                .epoch_lost
                .load(std::sync::atomic::Ordering::Acquire)
            || storage.descriptor != logical.desc
            || !native_matches_descriptor(storage)
        {
            return Err(WgpuRenderProgramError::StaleImport);
        }
    }
    // Validate completeness now, before a missing import could reject after local allocations.
    for resource in graph
        .resources()
        .iter()
        .filter(|resource| resource.is_imported())
    {
        graph.resource(&resource.version)?;
        if !program
            .0
            .imports
            .iter()
            .any(|entry| entry.version == resource.version)
        {
            return Err(WgpuRenderProgramError::StaleImport);
        }
    }
    Ok(())
}

fn native_matches_descriptor(storage: &ProgramResourceStorage) -> bool {
    match (&storage.descriptor, &storage.native) {
        (ResourceDescriptor::Texture { native, .. }, NativeResource::Texture { texture, view }) => {
            texture.size() == native.size
                && texture.dimension() == native.dimension
                && texture.format() == native.format
                && texture.mip_level_count() == native.mip_level_count
                && texture.sample_count() == native.sample_count
                && texture.usage() == native.usage
                && view.texture() == texture
        }
        (ResourceDescriptor::Buffer { native, .. }, NativeResource::Buffer(buffer)) => {
            buffer.size() == native.size && buffer.usage() == native.usage
        }
        _ => false,
    }
}

fn initialize(
    resources: WgpuOperationResources<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), WgpuRenderProgramError> {
    for write in &resources.operation.writes {
        let destination = resources.storage(&write.version)?;
        match (&write.initialization, &destination.native) {
            (Initialization::Clear(color), NativeResource::Texture { view, .. }) => {
                // A separate whole-attachment pass cannot inherit the Feature's viewport or
                // scissor. Store preserves the initialized contents for every later consumer.
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("nara_render_program_initialize"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(*color),
                            store: write.store_op(),
                        },
                    })],
                    ..Default::default()
                });
            }
            (Initialization::Zero, NativeResource::Buffer(buffer)) => {
                encoder.clear_buffer(buffer, 0, Some(buffer.size()));
            }
            (Initialization::Preserve(predecessor), _) => {
                let source = resources.storage(predecessor)?;
                match (&source.native, &destination.native) {
                    (
                        NativeResource::Texture { texture: input, .. },
                        NativeResource::Texture {
                            texture: output, ..
                        },
                    ) => {
                        encoder.copy_texture_to_texture(
                            input.as_image_copy(),
                            output.as_image_copy(),
                            output.size(),
                        );
                    }
                    (NativeResource::Buffer(input), NativeResource::Buffer(output)) => {
                        encoder.copy_buffer_to_buffer(input, 0, output, 0, output.size());
                    }
                    _ => return Err(WgpuRenderProgramError::ResourceKind),
                }
            }
            _ => return Err(GraphError::InvalidInitialization(write.version.index()).into()),
        }
    }
    Ok(())
}

fn resource_bytes(descriptor: &ResourceDescriptor) -> Result<u64, WgpuRenderProgramError> {
    match descriptor {
        ResourceDescriptor::Texture { native, .. } => {
            let texel_bytes = native
                .format
                .block_copy_size(None)
                .ok_or(GraphError::UnsupportedTextureFormat)?;
            u64::from(native.size.width)
                .checked_mul(u64::from(native.size.height))
                .and_then(|pixels| pixels.checked_mul(u64::from(texel_bytes)))
                .ok_or(WgpuRenderProgramError::ResourceCapacity)
        }
        ResourceDescriptor::Buffer { native, .. } => Ok(native.size),
    }
}

fn validate_device_size(
    descriptor: &ResourceDescriptor,
    limits: &wgpu::Limits,
) -> Result<(), WgpuRenderProgramError> {
    let (name, required, enabled) = match descriptor {
        ResourceDescriptor::Texture { native, .. } => (
            "max_texture_dimension_2d",
            u64::from(native.size.width.max(native.size.height)),
            u64::from(limits.max_texture_dimension_2d),
        ),
        ResourceDescriptor::Buffer { native, .. } => {
            ("max_buffer_size", native.size, limits.max_buffer_size)
        }
    };
    if required > enabled {
        return Err(WgpuRenderProgramError::DeviceResourceLimit {
            name,
            required,
            enabled,
        });
    }
    Ok(())
}

fn reserved<T>(count: usize) -> Result<Vec<T>, WgpuRenderProgramError> {
    count
        .checked_mul(size_of::<T>())
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or(WgpuRenderProgramError::ResourceCapacity)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
    Ok(values)
}

#[cfg(test)]
mod tests;
