//! Shared stock quad realization owned by the Host's backend and device epoch.

#[cfg(feature = "stock-sprite")]
use std::ops::Range;
use std::{mem::size_of, sync::Arc};

use crate::{
    GraphError, LimitKind, WgpuRenderError, WgpuRenderProgramError,
    budget::{InstanceBudget, InstanceReservation},
    output::OutputReservation,
    preparation::WgpuFeatureAllocation,
    quad::{
        WgpuMaterial2dLayouts, WgpuQuadBatch, WgpuQuadBatchBuffer, WgpuQuadPipeline,
        WgpuQuadPipelineDrawRef, WgpuQuadPipelineKey, admit_quad_instance_buffers,
        create_material_bind_group_layout, create_quad_batch_buffers, create_quad_pipeline,
        create_quad_texture_bind_group_layout, quad_batch_draw_stats,
    },
    submission::WgpuSubmissionReservation,
    texture::{
        WgpuCachedImageView, WgpuCachedTextureBinding, WgpuSpriteTextureCache,
        stock_texture_record_metadata_bytes,
    },
};
#[cfg(test)]
use crate::{PreparedSubmitterDraw, frame::WgpuLoweredFrameDraw};

pub(crate) const MAX_CACHED_QUAD_PIPELINES: usize = 64;

/// Shared quad native work. Domain facts and one-shot completion remain in the domain adapter.
#[derive(Debug, Default)]
pub(crate) struct StockQuadPrepared {
    pub(crate) pipelines: Vec<WgpuQuadPipelineDrawRef>,
    pub(crate) buffers: Vec<WgpuQuadBatchBuffer>,
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
    pub(crate) image_sprites: u32,
    pub(crate) instance_reservation: Option<InstanceReservation>,
}

impl StockQuadPrepared {
    /// One stock Feature owns the ordered attachment transaction. These ranges select the
    /// already prepared buffers; they do not create another writer or copy the attachment.
    #[cfg(feature = "stock-sprite")]
    pub(crate) fn encode_views(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        views: impl Iterator<
            Item = (
                nara_render::ViewportRect,
                nara_render::ViewLoadOp,
                Range<usize>,
            ),
        >,
    ) {
        for (viewport, load, buffers) in views {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nara_stock_feature"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: match load {
                            nara_render::ViewLoadOp::Clear(color) => wgpu::LoadOp::Clear(
                                crate::stock_policy::premultiplied_clear_color(color),
                            ),
                            nara_render::ViewLoadOp::Load => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_viewport(
                viewport.physical_x as f32,
                viewport.physical_y as f32,
                viewport.physical_width as f32,
                viewport.physical_height as f32,
                0.0,
                1.0,
            );
            crate::quad::draw_quad_batch_buffers(
                &mut pass,
                &self.pipelines,
                &self.buffers[buffers],
                viewport,
            );
        }
    }
}

/// Candidate-local immutable working-set collection. One stock adapter registers its exact
/// lowered slice once per operation; native preparation consumes those same captured batches.
pub(crate) struct StockImagePreflight {
    batches: Vec<Arc<[WgpuQuadBatch]>>,
    maximum: usize,
}

/// Additional engine-owned capacity for one candidate's captured stock work.
///
/// This counts inline records, Arc counters, automatic retention boxes and Vec capacity, using
/// actual Rust type sizes and encoded material uniforms. It excludes image pixels, other indirect
/// user payloads, WGPU driver memory,
/// allocator rounding and BTree node/allocation overhead. It is an envelope, not live memory.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StockPreparationCost {
    pub(crate) custody_entries: usize,
    pub(crate) modeled_metadata_bytes: usize,
}

impl StockPreparationCost {
    /// Borrows the actual batches across all Runtime, Editor or typed-program captures.
    /// Each nonempty batch may belong to a separate prepared draw; do not deduplicate by Arc,
    /// material or texture identity, since each occurrence can create its own native buffers.
    pub(crate) fn for_batches<'a>(
        batches: impl Iterator<Item = &'a WgpuQuadBatch>,
    ) -> Result<Self, WgpuRenderProgramError> {
        let overflow = || GraphError::LimitExceeded(LimitKind::MetadataBytes);
        let mut count = 0_usize;
        let mut extensions = 0_usize;
        let mut material_textures = 0_usize;
        let mut material_bytes = 0_usize;
        for batch in batches.filter(|batch| !batch.instances.is_empty()) {
            count = count.checked_add(1).ok_or_else(overflow)?;
            if let Some(material) = &batch.material_extension {
                extensions = extensions.checked_add(1).ok_or_else(overflow)?;
                material_textures = material_textures
                    .checked_add(material.textures.len())
                    .ok_or(GraphError::LimitExceeded(LimitKind::SubmissionResources))?;
                material_bytes = material_bytes
                    .checked_add(material.binding.uniform_bytes().len())
                    .ok_or_else(overflow)?;
            }
        }
        Self::for_resources(count, extensions, material_textures, material_bytes)
    }

    #[cfg(test)]
    fn for_counts(batches: usize, extensions: usize) -> Result<Self, WgpuRenderProgramError> {
        Self::for_resources(batches, extensions, extensions, 0)
    }

    fn for_resources(
        batches: usize,
        extensions: usize,
        material_textures: usize,
        material_bytes: usize,
    ) -> Result<Self, WgpuRenderProgramError> {
        let textures = batches
            .checked_add(material_textures)
            .ok_or(GraphError::LimitExceeded(LimitKind::SubmissionResources))?;
        let buffers = batches
            .checked_add(extensions)
            .ok_or(GraphError::LimitExceeded(LimitKind::SubmissionResources))?;
        let custody_entries = textures
            .checked_mul(3)
            .and_then(|count| {
                buffers
                    .checked_mul(2)
                    .and_then(|extra| count.checked_add(extra))
            })
            .ok_or(GraphError::LimitExceeded(LimitKind::SubmissionResources))?;
        if batches == 0 {
            return Ok(Self::default());
        }
        let overflow = || GraphError::LimitExceeded(LimitKind::MetadataBytes);
        let arc_counters = size_of::<usize>().checked_mul(2).ok_or_else(overflow)?;
        let mut bytes = 0_usize;
        bytes = bytes
            .checked_add(size_of::<StockQuadResources>())
            .ok_or_else(overflow)?;
        // Filtered/unique collections can start at RawVec's four-element minimum. A borrowed
        // iterator may span one separately owned draw per batch, so reserve that worst case.
        // Four slots per batch cover both the minimum and at most twice geometric growth.
        let capture_capacity = batches.checked_mul(4).ok_or_else(overflow)?;
        let pipeline_capacity = batches.checked_mul(2).ok_or_else(overflow)?.max(4);
        let mut buffer_records = size_of::<OutputReservation>()
            .checked_add(arc_counters)
            .ok_or_else(overflow)?;
        for record in [
            WgpuSubmissionReservation::modeled_retain_bytes::<WgpuFeatureAllocation>(),
            WgpuSubmissionReservation::modeled_retain_bytes::<(wgpu::Buffer, WgpuFeatureAllocation)>(
            ),
        ] {
            buffer_records = buffer_records.checked_add(record).ok_or_else(overflow)?;
        }
        let mut texture_records = stock_texture_record_metadata_bytes().ok_or_else(overflow)?;
        for record in [
            WgpuSubmissionReservation::modeled_retain_bytes::<WgpuFeatureAllocation>(),
            WgpuSubmissionReservation::modeled_retain_bytes::<wgpu::Texture>(),
            WgpuSubmissionReservation::modeled_retain_bytes::<WgpuCachedTextureBinding>().max(
                WgpuSubmissionReservation::modeled_retain_bytes::<WgpuCachedImageView>(),
            ),
        ] {
            texture_records = texture_records.checked_add(record).ok_or_else(overflow)?;
        }
        for (count, stride) in [
            (batches, size_of::<StockQuadPrepared>()),
            (batches, size_of::<Vec<WgpuQuadPipelineKey>>()),
            (capture_capacity, size_of::<WgpuQuadBatchBuffer>()),
            (capture_capacity, size_of::<WgpuQuadPipelineKey>()),
            (capture_capacity, size_of::<WgpuQuadPipelineDrawRef>()),
            (pipeline_capacity, size_of::<CachedQuadPipeline>()),
            (textures, texture_records),
            (buffers, buffer_records),
            (
                material_textures.checked_mul(4).ok_or_else(overflow)?,
                size_of::<WgpuCachedImageView>()
                    + size_of::<wgpu::BindGroupEntry<'_>>()
                    + size_of::<wgpu::BindGroupLayoutEntry>(),
            ),
            (extensions, size_of::<([u8; 32], wgpu::BindGroupLayout)>()),
            (
                extensions.checked_mul(4).ok_or_else(overflow)?,
                size_of::<wgpu::BindGroupEntry<'_>>() + size_of::<wgpu::BindGroupLayoutEntry>(),
            ),
            (material_bytes, 1),
        ] {
            let table = count
                .checked_mul(stride)
                .filter(|bytes| *bytes <= isize::MAX as usize)
                .ok_or_else(overflow)?;
            bytes = bytes.checked_add(table).ok_or_else(overflow)?;
        }
        Ok(Self {
            custody_entries,
            modeled_metadata_bytes: bytes,
        })
    }
}

impl StockImagePreflight {
    pub(crate) fn new(maximum: usize) -> Result<Self, crate::WgpuRenderProgramError> {
        let mut batches = Vec::new();
        batches
            .try_reserve_exact(maximum)
            .map_err(|_| crate::WgpuRenderProgramError::ResourceCapacity)?;
        Ok(Self { batches, maximum })
    }

    pub(crate) fn collect(
        &mut self,
        batches: &Arc<[WgpuQuadBatch]>,
    ) -> Result<(), crate::WgpuRenderProgramError> {
        if self.batches.len() >= self.maximum {
            return Err(crate::WgpuRenderProgramError::Graph(
                crate::GraphError::LimitExceeded(crate::LimitKind::Operations),
            ));
        }
        self.batches.push(Arc::clone(batches));
        Ok(())
    }

    pub(crate) fn validate(
        &self,
        resources: &StockQuadResources,
        limits: &wgpu::Limits,
    ) -> Result<(), WgpuRenderError> {
        let batches = self.batches.iter().flat_map(|batches| batches.iter());
        resources.preflight_material_device_limits(batches, limits)?;
        resources.preflight_image_residency(self.batches.iter().flat_map(|batches| batches.iter()))
    }

    /// Borrows every exact capture before any native allocation, state creation or upload.
    /// The caller admits compiled metadata plus this cost and reserves complete original custody.
    pub(crate) fn cost(&self) -> Result<StockPreparationCost, WgpuRenderProgramError> {
        StockPreparationCost::for_batches(self.batches.iter().flat_map(|batches| batches.iter()))
    }

    pub(crate) fn allocation_bytes(
        &self,
        resources: &StockQuadResources,
    ) -> Result<u64, WgpuRenderError> {
        resources.pending_allocation_bytes(self.batches.iter().flat_map(|batches| batches.iter()))
    }
}

#[derive(Debug)]
enum CachedQuadPipeline {
    Ready(WgpuQuadPipeline),
    Rejected {
        key: WgpuQuadPipelineKey,
        message: String,
    },
}

impl CachedQuadPipeline {
    fn key(&self) -> &WgpuQuadPipelineKey {
        match self {
            Self::Ready(pipeline) => &pipeline.key,
            Self::Rejected { key, .. } => key,
        }
    }
}

/// One shared cache and instance budget for all stock draws in this backend.
///
/// The backend clears epoch resources before replacing its device owner. Frame transaction
/// demand and texture frame maintenance retain their separate reset boundaries.
#[derive(Debug, Default)]
pub(crate) struct StockQuadResources {
    texture_layout: Option<wgpu::BindGroupLayout>,
    material_layouts: WgpuMaterial2dLayouts,
    pipelines: Vec<CachedQuadPipeline>,
    #[cfg(test)]
    pipeline_attempts: usize,
    textures: WgpuSpriteTextureCache,
    instance_budget: InstanceBudget,
}

impl StockQuadResources {
    pub(crate) fn pending_allocation_bytes<'a>(
        &self,
        batches: impl Iterator<Item = &'a WgpuQuadBatch> + Clone,
    ) -> Result<u64, WgpuRenderError> {
        let mut bytes = self.textures.pending_allocation_bytes(batches.clone())?;
        for batch in batches.filter(|batch| !batch.instances.is_empty()) {
            bytes = bytes
                .checked_add(
                    u64::try_from(batch.instances.len())
                        .ok()
                        .and_then(|count| {
                            count.checked_mul(size_of::<crate::quad::WgpuQuadInstance>() as u64)
                        })
                        .ok_or(crate::preparation::NativePreparationError::ResourceCapacity)?,
                )
                .ok_or(crate::preparation::NativePreparationError::ResourceCapacity)?;
            if let Some(material) = &batch.material_extension {
                let uniform = material.binding.uniform_bytes().len() as u64;
                let aligned = uniform
                    .checked_add(wgpu::COPY_BUFFER_ALIGNMENT - 1)
                    .map(|size| size & !(wgpu::COPY_BUFFER_ALIGNMENT - 1))
                    .ok_or(crate::preparation::NativePreparationError::ResourceCapacity)?;
                bytes = bytes
                    .checked_add(aligned)
                    .ok_or(crate::preparation::NativePreparationError::ResourceCapacity)?;
            }
        }
        Ok(bytes)
    }

    pub(crate) fn preflight_image_residency<'a>(
        &self,
        batches: impl Iterator<Item = &'a WgpuQuadBatch>,
    ) -> Result<(), WgpuRenderError> {
        self.textures
            .preflight_image_residency(batches)
            .map_err(WgpuRenderError::from)
    }

    pub(crate) fn preflight_material_device_limits<'a>(
        &self,
        batches: impl Iterator<Item = &'a WgpuQuadBatch>,
        limits: &wgpu::Limits,
    ) -> Result<(), WgpuRenderError> {
        for batch in batches.filter(|batch| !batch.instances.is_empty()) {
            crate::texture::validate_quad_texture_device_limits(&batch.texture, limits)?;
            if batch.material.texture != batch.texture.key() {
                return Err(crate::texture::WgpuTextureError::TextureIdentityMismatch {
                    expected: batch.material.texture,
                    actual: batch.texture.key(),
                }
                .into());
            }
            let Some(material) = &batch.material_extension else {
                continue;
            };
            crate::quad::validate_material_capture(material).map_err(WgpuRenderError::from)?;
            for (_, snapshot) in &material.textures {
                crate::texture::validate_image_device_limits(snapshot, limits)?;
            }
            let declaration = material.binding.generation().declaration();
            crate::quad::validate_material_device_limits(declaration, limits)?;
            if batch.material.alpha_mode != declaration.alpha_mode() {
                return Err(WgpuRenderError::ContentRejected {
                    message: format!(
                        "material {:?} capture disagrees with its admitted alpha mode",
                        declaration.id(),
                    ),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn reset_instance_budget(&mut self) {
        self.instance_budget = InstanceBudget::default();
    }

    pub(crate) fn instance_charges(&self) -> (u64, u64) {
        self.instance_budget.charges()
    }

    pub(crate) fn begin_frame(&mut self, frame_index: u64) {
        self.textures.begin_frame(frame_index);
    }

    pub(crate) fn clear(&mut self) {
        self.texture_layout = None;
        self.material_layouts.clear();
        self.pipelines.clear();
        self.textures.clear();
    }

    #[cfg(test)]
    pub(crate) fn texture_stats(&self) -> crate::WgpuRenderTextureCacheStats {
        self.textures.stats()
    }

    pub(crate) fn logical_texture_bytes(&self) -> u64 {
        self.textures.logical_texture_bytes()
    }

    /// Private legacy draw adapter for native/unit oracles; production uses `prepare_batches`.
    #[cfg(test)]
    pub(crate) fn prepare(
        &mut self,
        preparation: &mut crate::preparation::NativePreparation<'_>,
        check_device: impl Fn() -> Result<(), WgpuRenderError>,
        view_format: wgpu::TextureFormat,
        lowered: WgpuLoweredFrameDraw,
        frame_index: u64,
    ) -> Result<PreparedSubmitterDraw, WgpuRenderError> {
        let batches = crate::allocation::take_test_batches().unwrap_or(lowered.batches);
        #[cfg(feature = "stock-ui")]
        let ui_packets = lowered.ui_packets;
        let prepared = self.prepare_batches(
            preparation,
            check_device,
            view_format,
            &batches,
            frame_index,
        )?;
        Ok(PreparedSubmitterDraw {
            pipelines: prepared.pipelines,
            buffers: prepared.buffers,
            draw_calls: prepared.draw_calls,
            sprites: prepared.sprites,
            #[cfg(feature = "stock-ui")]
            image_sprites: prepared.image_sprites,
            #[cfg(feature = "stock-ui")]
            ui_packets,
            instance_reservation: prepared.instance_reservation,
        })
    }

    pub(crate) fn prepare_batches(
        &mut self,
        preparation: &mut crate::preparation::NativePreparation<'_>,
        check_device: impl Fn() -> Result<(), WgpuRenderError>,
        view_format: wgpu::TextureFormat,
        batches: &[WgpuQuadBatch],
        frame_index: u64,
    ) -> Result<StockQuadPrepared, WgpuRenderError> {
        let result = self.prepare_batches_inner(
            preparation,
            check_device,
            view_format,
            batches,
            frame_index,
        );
        self.trim_pipeline_cache();
        result
    }

    fn prepare_batches_inner(
        &mut self,
        preparation: &mut crate::preparation::NativePreparation<'_>,
        check_device: impl Fn() -> Result<(), WgpuRenderError>,
        view_format: wgpu::TextureFormat,
        batches: &[WgpuQuadBatch],
        frame_index: u64,
    ) -> Result<StockQuadPrepared, WgpuRenderError> {
        let stats = quad_batch_draw_stats(batches);
        if batches.is_empty() {
            return Ok(StockQuadPrepared::default());
        }
        check_device()?;
        self.preflight_material_device_limits(batches.iter(), &preparation.device().limits())?;
        preparation.preflight_allocation_bytes(self.pending_allocation_bytes(batches.iter())?)?;
        let required_bytes = admit_quad_instance_buffers(&batches).map_err(|error| {
            WgpuRenderError::ContentRejected {
                message: error.to_string(),
            }
        })?;
        let instance_reservation =
            self.instance_budget
                .reserve(required_bytes)
                .map_err(|error| WgpuRenderError::ContentRejected {
                    message: error.to_string(),
                })?;
        let pipeline_keys = crate::required_quad_pipeline_keys(&batches, view_format);
        let device = preparation.device();
        let texture_layout = self.ensure_texture_layout(device)?;
        for key in &pipeline_keys {
            self.ensure_pipeline(device, &check_device, key)?;
        }
        let pipelines = self.pipeline_draw_refs(&pipeline_keys)?;
        let buffers = create_quad_batch_buffers(
            preparation,
            &batches,
            &texture_layout,
            &self.material_layouts,
            &mut self.textures,
            frame_index,
        )
        .map_err(WgpuRenderError::from)?;
        Ok(StockQuadPrepared {
            pipelines,
            buffers,
            draw_calls: stats.draw_calls,
            sprites: stats.sprites,
            image_sprites: stats.image_sprites,
            instance_reservation: Some(instance_reservation),
        })
    }

    fn ensure_texture_layout(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<wgpu::BindGroupLayout, WgpuRenderError> {
        if let Some(layout) = &self.texture_layout {
            return Ok(layout.clone());
        }
        let layout = create_quad_texture_bind_group_layout(device)?;
        self.texture_layout = Some(layout.clone());
        Ok(layout)
    }

    fn ensure_material_layout(
        &mut self,
        device: &wgpu::Device,
        declaration: &nara_material::Material2dDeclaration,
    ) -> Result<wgpu::BindGroupLayout, WgpuRenderError> {
        let digest = declaration.binding_digest();
        if let Some(layout) = self.material_layouts.get(&digest) {
            return Ok(layout.clone());
        }
        let layout = create_material_bind_group_layout(device, declaration)?;
        self.material_layouts.insert(digest, layout.clone());
        Ok(layout)
    }

    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        check_device: &impl Fn() -> Result<(), WgpuRenderError>,
        key: &WgpuQuadPipelineKey,
    ) -> Result<(), WgpuRenderError> {
        check_device()?;
        if let Some(index) = self.pipelines.iter().position(|cached| cached.key() == key) {
            let cached = self.pipelines.remove(index);
            let result = match &cached {
                CachedQuadPipeline::Ready(_) => Ok(()),
                CachedQuadPipeline::Rejected { message, .. } => {
                    Err(WgpuRenderError::ContentRejected {
                        message: message.clone(),
                    })
                }
            };
            self.pipelines.push(cached);
            return result;
        }
        let texture_layout = self.ensure_texture_layout(device)?;
        let material_layout = key
            .material
            .as_ref()
            .map(|material| self.ensure_material_layout(device, material.declaration()))
            .transpose()?;
        #[cfg(test)]
        {
            self.pipeline_attempts += 1;
        }
        let result = create_quad_pipeline(
            device,
            key.clone(),
            &texture_layout,
            material_layout.as_ref(),
        );
        // Device loss is a namespace failure even if it also invalidated a shader handle.
        check_device()?;
        match result {
            Ok(pipeline) => {
                self.pipelines.push(CachedQuadPipeline::Ready(pipeline));
                Ok(())
            }
            Err(WgpuRenderError::ContentRejected { message }) => {
                self.pipelines.push(CachedQuadPipeline::Rejected {
                    key: key.clone(),
                    message: message.clone(),
                });
                Err(WgpuRenderError::ContentRejected { message })
            }
            Err(error) => Err(error),
        }
    }

    /// Limits cross-call cache custody after prepared draws have cloned their native resources.
    /// A single admitted working set may exceed this cache; original submissions remain owners.
    fn trim_pipeline_cache(&mut self) {
        let excess = self
            .pipelines
            .len()
            .saturating_sub(MAX_CACHED_QUAD_PIPELINES);
        self.pipelines.drain(..excess);
        let pipelines = &self.pipelines;
        self.material_layouts.retain(|digest, _| {
            pipelines.iter().any(|cached| {
                cached
                    .key()
                    .material
                    .as_ref()
                    .is_some_and(|material| material.declaration().binding_digest() == *digest)
            })
        });
    }

    fn pipeline_draw_refs(
        &self,
        keys: &[WgpuQuadPipelineKey],
    ) -> Result<Vec<WgpuQuadPipelineDrawRef>, WgpuRenderError> {
        keys.iter()
            .map(|key| {
                let pipeline = self
                    .pipelines
                    .iter()
                    .find_map(|cached| match cached {
                        CachedQuadPipeline::Ready(pipeline) if pipeline.key == *key => {
                            Some(pipeline)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| WgpuRenderError::QuadPipelineMissing {
                        format: format!("{:?}", key.format),
                        alpha_mode: format!("{:?}", key.alpha_mode),
                    })?;
                Ok(WgpuQuadPipelineDrawRef {
                    alpha_mode: key.alpha_mode,
                    material: key.material.clone(),
                    pipeline: pipeline.pipeline.clone(),
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn reserve_instance_buffer_bytes(
        &self,
        bytes: u64,
    ) -> Result<crate::budget::InstanceReservation, crate::budget::InstanceBudgetError> {
        self.instance_budget.reserve(bytes)
    }

    #[cfg(test)]
    pub(crate) fn set_instance_budget_limit(&mut self, maximum: u64) {
        self.instance_budget = InstanceBudget::with_limit(maximum);
    }

    #[cfg(test)]
    pub(crate) fn textures_mut(&mut self) -> &mut WgpuSpriteTextureCache {
        &mut self.textures
    }

    #[cfg(test)]
    pub(crate) fn pipeline_stats(&self) -> (usize, usize, usize) {
        (
            self.pipeline_attempts,
            self.pipelines.len(),
            self.material_layouts.len(),
        )
    }
}

#[cfg(test)]
mod material_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::WgpuDeviceOwner;
    use crate::quad::{
        QuadShaderSourceGuard, WgpuMaterial2d, WgpuMaterial2dPipelineKey, WgpuQuadInstance,
        WgpuQuadMaterialKey, WgpuQuadTexture, WgpuQuadTextureKey, WgpuScissorRect,
        draw_quad_batch_buffers,
    };
    use crate::readback_tests::{
        ReadbackGpu, TEXTURE_MATERIAL_FRAGMENT, image_versions, image_versions_with_handle,
        material_binding, material_generation, readback_pixel, readback_viewport, render_to_rgba,
    };
    use nara_material::{AlphaMode2d, SamplerDescriptor};

    fn cost_test_batch() -> WgpuQuadBatch {
        WgpuQuadBatch {
            material: WgpuQuadMaterialKey {
                texture: WgpuQuadTextureKey::Fallback,
                sampler: SamplerDescriptor::NEAREST_CLAMP,
                alpha_mode: AlphaMode2d::Opaque,
            },
            texture: WgpuQuadTexture::Fallback,
            scissor: None,
            instances: vec![<WgpuQuadInstance as bytemuck::Zeroable>::zeroed()],
            counts_as_sprites: true,
            material_extension: None,
        }
    }

    #[test]
    fn stock_preparation_cost_is_zero_for_empty_captures_and_batches() {
        assert_eq!(
            StockPreparationCost::for_batches(std::iter::empty()).unwrap(),
            StockPreparationCost::default(),
        );
        let mut preflight = StockImagePreflight::new(2).unwrap();
        assert_eq!(preflight.cost().unwrap(), StockPreparationCost::default());
        preflight.collect(&Arc::from([])).unwrap();
        let mut empty = cost_test_batch();
        empty.instances.clear();
        preflight.collect(&Arc::from([empty])).unwrap();
        assert_eq!(preflight.cost().unwrap(), StockPreparationCost::default());
    }

    #[test]
    fn stock_preparation_cost_admits_five_automatic_entries_per_fallback_batch() {
        let batch = cost_test_batch();
        let cost = StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap();
        assert_eq!(cost.custody_entries, 5);
        assert!(
            cost.modeled_metadata_bytes
                >= size_of::<StockQuadPrepared>()
                    + 4 * size_of::<WgpuQuadBatchBuffer>()
                    + 4 * size_of::<WgpuQuadPipelineKey>()
                    + 4 * size_of::<WgpuQuadPipelineDrawRef>()
                    + 4 * size_of::<CachedQuadPipeline>()
                    + stock_texture_record_metadata_bytes().unwrap(),
            "the envelope includes engine records and RawVec's four-element minimum",
        );
    }

    #[test]
    fn stock_preparation_cost_adds_five_entries_for_a_material_extension() {
        let (handle, [image, _]) = image_versions_with_handle(1, 1);
        let mut batch = cost_test_batch();
        let primary = StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap();
        batch.material_extension = Some(WgpuMaterial2d {
            binding: material_binding(
                &material_generation(
                    "test.stock-preparation-cost",
                    AlphaMode2d::Opaque,
                    TEXTURE_MATERIAL_FRAGMENT,
                ),
                [0.0; 4],
                handle,
            ),
            textures: vec![(1, image)],
        });
        let extended = StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap();
        assert_eq!(extended.custody_entries, 10);
        assert!(extended.modeled_metadata_bytes > primary.modeled_metadata_bytes);
    }

    #[test]
    fn stock_preflight_cost_counts_every_capture_occurrence_and_uses_the_shared_calculator() {
        let batches: Arc<[WgpuQuadBatch]> = Arc::from([cost_test_batch()]);
        let mut preflight = StockImagePreflight::new(3).unwrap();
        for _ in 0..3 {
            preflight.collect(&batches).unwrap();
        }
        let cost = preflight.cost().unwrap();
        assert_eq!(cost.custody_entries, 15);
        assert_eq!(
            cost,
            StockPreparationCost::for_batches(std::iter::repeat_n(batches.as_ref(), 3).flatten(),)
                .unwrap(),
        );
        assert_eq!(Arc::strong_count(&batches), 4, "cost only borrows captures");
    }

    #[test]
    fn stock_preparation_cost_exposes_complete_large_candidate_custody_before_gpu_work() {
        let batch = cost_test_batch();
        let cost = StockPreparationCost::for_batches(std::iter::repeat_n(&batch, 20_000)).unwrap();
        assert_eq!(cost.custody_entries, 100_000);
        assert!(cost.custody_entries > crate::submission::MAX_SUBMISSION_RESOURCES);
        // Cost is candidate-local; another candidate starts with the same five-entry demand.
        assert_eq!(
            StockPreparationCost::for_batches(std::iter::once(&batch))
                .unwrap()
                .custody_entries,
            5,
        );
    }

    #[test]
    fn stock_preparation_cost_checks_custody_addition_and_multiplication_overflow() {
        for (batches, extensions) in [(usize::MAX, 1), (usize::MAX / 5 + 1, 0)] {
            assert!(matches!(
                StockPreparationCost::for_counts(batches, extensions),
                Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
                    LimitKind::SubmissionResources,
                )))
            ));
        }
    }

    #[test]
    fn stock_preparation_cost_checks_metadata_capacity_overflow_separately() {
        assert!(matches!(
            StockPreparationCost::for_counts(usize::MAX / 5, 0),
            Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
                LimitKind::MetadataBytes,
            )))
        ));
    }

    #[test]
    fn stock_preparation_metadata_excludes_image_pixels_and_indirect_instance_payloads() {
        let mut batch = cost_test_batch();
        let mut costs = Vec::new();
        for (width, instances) in [(1, 1), (64, 64)] {
            let [image, _] = image_versions(width, 1);
            batch.material.texture = WgpuQuadTextureKey::Image(image.identity().key());
            batch.texture = WgpuQuadTexture::Image(image);
            batch.instances.resize(instances, batch.instances[0]);
            costs.push(StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap());
        }
        assert_eq!(costs[0], costs[1]);
    }

    #[test]
    fn permanent_pipeline_rejection_is_cached_per_device_and_valid_material_renders() {
        use crate::allocation::AllocationFailureGuard;

        let bad_generation = material_generation(
            "test.pipeline-rejection",
            AlphaMode2d::Opaque,
            &format!("{TEXTURE_MATERIAL_FRAGMENT}\n"),
        );
        let good_generation = material_generation(
            "test.pipeline-rejection",
            AlphaMode2d::Opaque,
            TEXTURE_MATERIAL_FRAGMENT,
        );
        let bad_material = WgpuMaterial2dPipelineKey::new(&bad_generation);
        let good_material = WgpuMaterial2dPipelineKey::new(&good_generation);
        let _native_rejection = QuadShaderSourceGuard::invalid(&bad_material);
        let gpu = ReadbackGpu::new();
        let mut owner = WgpuDeviceOwner::default();
        owner.install_probe_device(gpu.device.clone(), gpu.queue.clone());
        let mut resources = StockQuadResources::default();
        let render =
            |resources: &mut StockQuadResources, batches: &[WgpuQuadBatch], frame_index| {
                resources.begin_frame(frame_index);
                resources.reset_instance_budget();
                gpu.render_to_rgba(|_, _, format, output_view, encoder, preparation| {
                    let prepared = resources.prepare_batches(
                        preparation,
                        || owner.fail_if_device_lost(),
                        format,
                        batches,
                        frame_index,
                    )?;
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("nara_pipeline_rejection_recovery"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: output_view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                        draw_quad_batch_buffers(
                            &mut pass,
                            &prepared.pipelines,
                            &prepared.buffers,
                            readback_viewport(),
                        );
                    }
                    Ok(format)
                })
            };
        let mut ordinary = cost_test_batch();
        ordinary.instances[0] = WgpuQuadInstance {
            center: [0.0; 2],
            x_axis: [1.0, 0.0],
            y_axis: [0.0, 1.0],
            color: [0.0, 0.0, 1.0, 1.0],
            uv_min: [0.0; 2],
            uv_size: [1.0; 2],
            texture_mode: 0,
            shape_mode: 0,
            rect_size: [1.0; 2],
            corner_radius: 0.0,
            border_width: 0.0,
        };
        let failure = AllocationFailureGuard::at("nara_material_2d_bind_group_layout");
        let (pixels, format) = render(&mut resources, std::slice::from_ref(&ordinary), 1).unwrap();
        assert_eq!(readback_pixel(&pixels, 32, 32), &[0, 0, 255, 255]);
        assert!(!failure.was_consumed());
        assert!(resources.material_layouts.is_empty());
        assert_eq!(resources.pipeline_attempts, 1);
        assert_eq!(resources.pipelines.len(), 1);

        let (handle, [red, green]) = image_versions_with_handle(1, 1);
        let mut custom = ordinary.clone();
        custom.instances[0].color = [1.0; 4];
        custom.material_extension = Some(WgpuMaterial2d {
            binding: material_binding(&good_generation, [1.0; 4], handle),
            textures: vec![(1, green)],
        });
        let before = resources.texture_stats();
        assert!(matches!(
            render(&mut resources, std::slice::from_ref(&custom), 2),
            Err(WgpuRenderError::UploadDeferred { message })
                if message.contains("nara_material_2d_bind_group_layout")
                    && message.contains("injected allocation failure")
        ));
        assert!(failure.was_consumed());
        assert!(resources.material_layouts.is_empty());
        assert_eq!(resources.pipeline_attempts, 1);
        assert_eq!(resources.pipelines.len(), 1);
        assert_eq!(resources.texture_stats(), before);
        assert_eq!(resources.instance_charges(), (0, 0));
        drop(failure);

        let bad_key = WgpuQuadPipelineKey {
            format,
            alpha_mode: AlphaMode2d::Opaque,
            material: Some(bad_material),
        };
        for _ in 1..=4 {
            resources.reset_instance_budget();
            assert!(matches!(
                resources.ensure_pipeline(&gpu.device, &|| owner.fail_if_device_lost(), &bad_key),
                Err(WgpuRenderError::ContentRejected { .. })
            ));
        }
        assert_eq!(resources.pipeline_attempts, 2);
        assert_eq!(resources.pipelines.len(), 2);
        let good_key = WgpuQuadPipelineKey {
            material: Some(good_material),
            ..bad_key.clone()
        };
        for _ in 5..=8 {
            resources.reset_instance_budget();
            resources
                .ensure_pipeline(&gpu.device, &|| owner.fail_if_device_lost(), &good_key)
                .unwrap();
        }
        assert_eq!(resources.pipeline_attempts, 3);

        let (pixels, _) = render(&mut resources, std::slice::from_ref(&custom), 3).unwrap();
        assert_eq!(readback_pixel(&pixels, 32, 32), &[0, 255, 0, 255]);
        assert!(!resources.material_layouts.is_empty());
        assert_eq!(resources.pipeline_attempts, 3);
        assert_eq!(resources.pipelines.len(), 3);
        let stats = resources.texture_stats();
        assert_eq!(stats.uploaded_images + stats.reuploaded_images, 1);
        assert_eq!(stats.image_textures, 1);

        let mut repeated = [ordinary, custom.clone(), custom];
        for (batch, (x, width)) in repeated.iter_mut().zip([(0, 21), (21, 21), (42, 22)]) {
            batch.scissor = Some(WgpuScissorRect {
                x,
                y: 0,
                width,
                height: 64,
            });
        }
        repeated[1].material_extension.as_mut().unwrap().binding =
            material_binding(&good_generation, [1.0, 0.0, 1.0, 1.0], handle);
        repeated[2].material_extension.as_mut().unwrap().textures = vec![(1, red)];
        let reuse_failure = AllocationFailureGuard::at("nara_material_2d_bind_group_layout");
        for frame_index in 4..=5 {
            let (pixels, _) = render(&mut resources, &repeated, frame_index).unwrap();
            assert_eq!(readback_pixel(&pixels, 10, 32), &[0, 0, 255, 255]);
            assert_eq!(readback_pixel(&pixels, 32, 32), &[0, 0, 0, 255]);
            assert_eq!(readback_pixel(&pixels, 53, 32), &[255, 0, 0, 255]);
            assert!(!reuse_failure.was_consumed());
            assert_eq!(resources.pipeline_attempts, 3);
            assert_eq!(resources.pipelines.len(), 3);
            let stats = resources.texture_stats();
            assert_eq!(stats.image_textures, 2);
            assert_eq!(stats.fallback_bindings, 1);
            if frame_index == 5 {
                assert_eq!(stats.uploaded_images + stats.reuploaded_images, 0);
                assert_eq!(stats.reused_images, 2);
            }
        }

        assert!(!reuse_failure.was_consumed());
        drop(reuse_failure);
        resources.clear();
        owner.clear();
        assert!(resources.texture_layout.is_none());
        assert!(resources.material_layouts.is_empty());
        assert!(resources.pipelines.is_empty());
        assert_eq!(resources.logical_texture_bytes(), 0);
        assert_eq!(gpu.budget.retained(), 0);
        drop(gpu);
        render_to_rgba(|device, queue, _, _, _, _| {
            owner.install_probe_device(device.clone(), queue.clone());
            for _ in 0..3 {
                assert!(matches!(
                    resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), &bad_key,),
                    Err(WgpuRenderError::ContentRejected { .. })
                ));
            }
            assert_eq!(
                resources.pipeline_attempts, 4,
                "replacement device must revalidate once"
            );
            let mut replacement_owner = WgpuDeviceOwner::default();
            replacement_owner.install_probe_device(device.clone(), queue.clone());
            let mut replacement = StockQuadResources::default();
            assert_ne!(replacement_owner.instance_id(), owner.instance_id());
            assert!(matches!(
                replacement.ensure_pipeline(
                    device,
                    &|| replacement_owner.fail_if_device_lost(),
                    &bad_key,
                ),
                Err(WgpuRenderError::ContentRejected { .. })
            ));
            assert_eq!(
                replacement.pipeline_attempts, 1,
                "replacement owner must revalidate"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn pipeline_allocation_failure_is_retryable_and_device_loss_is_not_content_rejection() {
        use crate::quad::QUAD_PIPELINE_ALLOCATION_FAILURE;

        render_to_rgba(|device, queue, format, _, _, _| {
            let mut owner = WgpuDeviceOwner::default();
            owner.install_probe_device(device.clone(), queue.clone());
            let mut resources = StockQuadResources::default();
            let material = WgpuMaterial2dPipelineKey::new(&material_generation(
                "test.pipeline-allocation",
                AlphaMode2d::Opaque,
                TEXTURE_MATERIAL_FRAGMENT,
            ));
            let _native_rejection = QuadShaderSourceGuard::invalid(&material);
            let key = WgpuQuadPipelineKey {
                format,
                alpha_mode: AlphaMode2d::Opaque,
                material: Some(material),
            };
            // Allocation failure takes priority over WGSL rejection and remains retryable.
            QUAD_PIPELINE_ALLOCATION_FAILURE.with(|failure| failure.set(true));
            assert!(matches!(
                resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), &key,),
                Err(WgpuRenderError::UploadDeferred { .. })
            ));
            assert!(resources.pipelines.is_empty());
            assert_eq!(resources.pipeline_attempts, 1);
            assert!(matches!(
                resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), &key,),
                Err(WgpuRenderError::ContentRejected { .. })
            ));
            assert_eq!(resources.pipeline_attempts, 2);
            owner.mark_probe_lost();
            assert_eq!(
                resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), &key,),
                Err(WgpuRenderError::DeviceLost)
            );
            assert_eq!(resources.pipeline_attempts, 2);
            resources.clear();
            assert!(resources.pipelines.is_empty());
            Ok(())
        })
        .unwrap();
    }
}
