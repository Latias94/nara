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
    NativeFeatureStates, ProgramOperationReport, WgpuFeatureAllocation, WgpuNativeResources,
    WgpuProgramBuffer, WgpuProgramTexture, WgpuRenderProgram, WgpuRenderProgramError,
    resources::{NativeResource, ProgramResourceStorage, require_descriptor},
};

/// Capability-only inspection before native resource allocation, state creation or upload.
/// Every captured operation completes this phase before the first operation starts preparation.
pub struct WgpuFeaturePreflightContext<'a> {
    limits: &'a wgpu::Limits,
    features: wgpu::Features,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    stock_images: &'a mut crate::stock_quad::StockImagePreflight,
}

impl WgpuFeaturePreflightContext<'_> {
    #[must_use]
    pub fn limits(&self) -> &wgpu::Limits {
        self.limits
    }

    #[must_use]
    pub fn features(&self) -> wgpu::Features {
        self.features
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    pub(crate) fn collect_stock_images(
        &mut self,
        batches: &Arc<[crate::quad::WgpuQuadBatch]>,
    ) -> Result<(), WgpuRenderProgramError> {
        self.stock_images.collect(batches)
    }
}

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

/// Native preparation under the original Host's pre-upload resource custody.
///
/// Queue uploads become visible before the submitted command buffer executes. Use them for
/// adapter-private inputs; shared graph writes still execute their declared initialization before
/// Feature encoding. The Host observes native validation and owns the submission attempt.
pub struct WgpuFeaturePrepareContext<'a> {
    preparation: crate::preparation::NativePreparation<'a>,
    resources: WgpuOperationResources<'a>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    stock_resources: &'a mut crate::stock_quad::StockQuadResources,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    instance_reservations: &'a mut Vec<crate::budget::InstanceReservation>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    epoch_lost: &'a std::sync::atomic::AtomicBool,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    frame_index: u64,
}

impl<'a> WgpuFeaturePrepareContext<'a> {
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    pub(crate) fn prepare_stock_draw(
        &mut self,
        batches: &Arc<[crate::quad::WgpuQuadBatch]>,
        format: wgpu::TextureFormat,
    ) -> Result<crate::stock_quad::StockQuadPrepared, WgpuRenderProgramError> {
        let device = self.preparation.device();
        let lost = self.epoch_lost;
        let mut prepared = self.stock_resources.prepare_batches(
            &mut self.preparation,
            || {
                let polled = device.poll(wgpu::PollType::Poll);
                if lost.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(crate::WgpuRenderError::DeviceLost);
                }
                polled
                    .map(|_| ())
                    .map_err(|_| crate::WgpuRenderError::TelemetryCompletionPollFailed)
            },
            format,
            batches,
            self.frame_index,
        )?;
        if let Some(reservation) = prepared.instance_reservation.take() {
            // One exact stock operation can reserve once. This Vec was admitted before uploads.
            if self.instance_reservations.len() == self.instance_reservations.capacity() {
                return Err(GraphError::LimitExceeded(crate::LimitKind::Operations).into());
            }
            self.instance_reservations.push(reservation);
        }
        Ok(prepared)
    }
    #[must_use]
    pub fn device(&self) -> &'a wgpu::Device {
        self.preparation.device()
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
        self.preparation
            .upload_buffer(destination, offset, data)
            .map_err(Into::into)
    }

    /// Retains the actual destination texture before the original queue can accept the upload.
    pub fn upload_texture(
        &mut self,
        destination: wgpu::TexelCopyTextureInfo<'_>,
        data: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) -> Result<(), WgpuRenderProgramError> {
        self.preparation
            .upload_texture(destination, data, layout, size)
            .map_err(Into::into)
    }

    /// Keeps native resources and their accounting alive through this original submission use.
    pub fn retain<T: WgpuNativeResources>(
        &mut self,
        value: T,
    ) -> Result<(), WgpuRenderProgramError> {
        self.preparation.retain(value).map_err(Into::into)
    }

    /// Reserves from the same retained-output budget as local graph resources.
    pub fn reserve_bytes(
        &mut self,
        bytes: u64,
    ) -> Result<WgpuFeatureAllocation, WgpuRenderProgramError> {
        self.preparation.reserve_bytes(bytes).map_err(Into::into)
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

#[derive(Default)]
pub(crate) struct FeaturePreparationAttempt {
    pub(crate) touched_instances: Vec<u64>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    instance_reservations: Vec<crate::budget::InstanceReservation>,
}

impl FeaturePreparationAttempt {
    pub(crate) fn commit_instance_reservations(&mut self) {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        for reservation in &mut self.instance_reservations {
            reservation.commit();
        }
    }
}

pub(crate) fn prepare_and_encode(
    program: &WgpuRenderProgram,
    namespace: &WgpuDeviceNamespace,
    producer: WgpuRenderProducerId,
    states: &mut NativeFeatureStates,
    budget: &Arc<OutputBudget>,
    reservation: &mut WgpuSubmissionReservation,
    feature_attempt: &mut FeaturePreparationAttempt,
    _host_frame_index: u64,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    stock_resources: &mut crate::stock_quad::StockQuadResources,
) -> Result<EncodedProgram, WgpuRenderProgramError> {
    let compiled = &program.0;
    let graph = &compiled.graph;
    feature_attempt.touched_instances = reserved(compiled.operations.len())?;
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    {
        feature_attempt.instance_reservations = reserved(compiled.operations.len())?;
    }
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

    let enabled_limits = namespace.device.limits();
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    let mut stock_images = crate::stock_quad::StockImagePreflight::new(compiled.operations.len())?;
    let mut preflight = WgpuFeaturePreflightContext {
        limits: &enabled_limits,
        features: namespace.device.features(),
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        stock_images: &mut stock_images,
    };
    for operation in graph.operations() {
        let original = compiled
            .operations
            .iter()
            .find(|entry| entry.key == operation.key)
            .ok_or(WgpuRenderProgramError::FeatureState)?;
        if let Some(capture) = &original.capture {
            capture.preflight(&mut preflight)?;
        }
    }
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    stock_images.validate(stock_resources)?;

    let automatic_entries = graph
        .resources()
        .len()
        .checked_add(1)
        .and_then(|count| {
            compiled
                .operations
                .len()
                .checked_mul(3)
                .and_then(|operations| count.checked_add(operations))
        })
        .ok_or(GraphError::LimitExceeded(
            crate::LimitKind::SubmissionResources,
        ))?;
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    let automatic_entries = {
        let cost = stock_images.cost()?;
        compiled
            .modeled_metadata_bytes
            .checked_add(cost.modeled_metadata_bytes)
            .filter(|bytes| *bytes <= compiled.max_modeled_metadata_bytes)
            .ok_or(GraphError::LimitExceeded(crate::LimitKind::MetadataBytes))?;
        automatic_entries
            .checked_add(cost.custody_entries)
            .ok_or(GraphError::LimitExceeded(
                crate::LimitKind::SubmissionResources,
            ))?
    };
    reservation.reserve_entries(automatic_entries)?;

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
        feature_attempt
            .touched_instances
            .push(capture.identity().id);
        let prepared = capture.prepare(
            states,
            namespace.device_epoch,
            &mut WgpuFeaturePrepareContext {
                preparation: crate::preparation::NativePreparation::new(
                    &namespace.device,
                    &namespace.queue,
                    budget,
                    reservation,
                ),
                resources: scoped,
                #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
                stock_resources,
                #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
                instance_reservations: &mut feature_attempt.instance_reservations,
                #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
                epoch_lost: &namespace.epoch_lost,
                #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
                frame_index: _host_frame_index,
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
