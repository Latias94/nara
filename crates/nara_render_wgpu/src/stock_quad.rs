//! Shared stock quad realization owned by the Host's backend and device epoch.

use std::{mem::size_of, sync::Arc};

use crate::{
    GraphError, LimitKind, PreparedSubmitterDraw, WgpuRenderError, WgpuRenderProgramError,
    budget::{InstanceBudget, InstanceReservation},
    frame::WgpuLoweredFrameDraw,
    output::OutputReservation,
    preparation::WgpuFeatureAllocation,
    quad::{
        WgpuQuadBatch, WgpuQuadBatchBuffer, WgpuQuadPipeline, WgpuQuadPipelineDrawRef,
        WgpuQuadPipelineKey, admit_quad_instance_buffers, create_material_bind_group_layout,
        create_quad_batch_buffers, create_quad_pipeline, create_quad_texture_bind_group_layout,
        quad_batch_draw_stats,
    },
    submission::WgpuSubmissionReservation,
    texture::{
        WgpuCachedImageView, WgpuCachedTextureBinding, WgpuSpriteTextureCache,
        stock_texture_record_metadata_bytes,
    },
};

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
    pub(crate) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        viewport: nara_render::ViewportRect,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara_stock_feature"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
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
        crate::quad::draw_quad_batch_buffers(&mut pass, &self.pipelines, &self.buffers, viewport);
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
/// actual Rust type sizes. It excludes image pixels, indirect user payloads, WGPU driver memory,
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
        for batch in batches.filter(|batch| !batch.instances.is_empty()) {
            count = count.checked_add(1).ok_or_else(overflow)?;
            if batch.material_extension.is_some() {
                extensions = extensions.checked_add(1).ok_or_else(overflow)?;
            }
        }
        Self::for_counts(count, extensions)
    }

    fn for_counts(batches: usize, extensions: usize) -> Result<Self, WgpuRenderProgramError> {
        // Texture allocation guard + upload + cached binding/view, followed by buffer guard +
        // the actual buffer/guard pair. An extension repeats this for its image and uniform.
        let textures = batches
            .checked_add(extensions)
            .ok_or(GraphError::LimitExceeded(LimitKind::SubmissionResources))?;
        let custody_entries = textures
            .checked_mul(5)
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
            (textures, buffer_records),
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

    pub(crate) fn validate(&self, resources: &StockQuadResources) -> Result<(), WgpuRenderError> {
        resources.preflight_image_residency(self.batches.iter().flat_map(|batches| batches.iter()))
    }

    /// Borrows every exact capture before any native allocation, state creation or upload.
    /// The caller admits compiled metadata plus this cost and reserves complete original custody.
    pub(crate) fn cost(&self) -> Result<StockPreparationCost, WgpuRenderProgramError> {
        StockPreparationCost::for_batches(self.batches.iter().flat_map(|batches| batches.iter()))
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
    fn key(&self) -> WgpuQuadPipelineKey {
        match self {
            Self::Ready(pipeline) => pipeline.key,
            Self::Rejected { key, .. } => *key,
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
    material_layout: Option<wgpu::BindGroupLayout>,
    pipelines: Vec<CachedQuadPipeline>,
    #[cfg(test)]
    pipeline_attempts: usize,
    textures: WgpuSpriteTextureCache,
    instance_budget: InstanceBudget,
}

impl StockQuadResources {
    pub(crate) fn preflight_image_residency<'a>(
        &self,
        batches: impl Iterator<Item = &'a WgpuQuadBatch>,
    ) -> Result<(), WgpuRenderError> {
        self.textures
            .preflight_image_residency(batches)
            .map_err(WgpuRenderError::from)
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
        self.material_layout = None;
        self.pipelines.clear();
        self.textures.clear();
    }

    pub(crate) fn texture_count(&self) -> usize {
        self.textures.image_count()
    }

    pub(crate) fn texture_stats(&self) -> crate::WgpuRenderTextureCacheStats {
        self.textures.stats()
    }

    pub(crate) fn logical_texture_bytes(&self) -> u64 {
        self.textures.logical_texture_bytes()
    }

    /// Realizes the captured draw against the caller's exact native namespace.
    ///
    /// The read-only health check retains the original epoch's pipeline failure precedence.
    pub(crate) fn prepare(
        &mut self,
        preparation: &mut crate::preparation::NativePreparation<'_>,
        check_device: impl Fn() -> Result<(), WgpuRenderError>,
        view_format: wgpu::TextureFormat,
        lowered: WgpuLoweredFrameDraw,
        frame_index: u64,
    ) -> Result<PreparedSubmitterDraw, WgpuRenderError> {
        #[cfg(test)]
        let batches = crate::allocation::take_test_batches().unwrap_or(lowered.batches);
        #[cfg(not(test))]
        let batches = lowered.batches;
        #[cfg(feature = "ui-submitter")]
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
            #[cfg(feature = "ui-submitter")]
            image_sprites: prepared.image_sprites,
            #[cfg(feature = "ui-submitter")]
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
        let stats = quad_batch_draw_stats(batches);
        if batches.is_empty() {
            return Ok(StockQuadPrepared::default());
        }
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
        let material_layout = self.ensure_material_layout(device)?;
        for key in &pipeline_keys {
            self.ensure_pipeline(device, &check_device, *key)?;
        }
        let pipelines = self.pipeline_draw_refs(&pipeline_keys)?;
        let buffers = create_quad_batch_buffers(
            preparation,
            &batches,
            &texture_layout,
            &material_layout,
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
    ) -> Result<wgpu::BindGroupLayout, WgpuRenderError> {
        if let Some(layout) = &self.material_layout {
            return Ok(layout.clone());
        }
        let layout = create_material_bind_group_layout(device)?;
        self.material_layout = Some(layout.clone());
        Ok(layout)
    }

    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        check_device: &impl Fn() -> Result<(), WgpuRenderError>,
        key: WgpuQuadPipelineKey,
    ) -> Result<(), WgpuRenderError> {
        check_device()?;
        if let Some(cached) = self.pipelines.iter().find(|cached| cached.key() == key) {
            return match cached {
                CachedQuadPipeline::Ready(_) => Ok(()),
                CachedQuadPipeline::Rejected { message, .. } => {
                    Err(WgpuRenderError::ContentRejected {
                        message: message.clone(),
                    })
                }
            };
        }
        let texture_layout = self.ensure_texture_layout(device)?;
        let material_layout = self.ensure_material_layout(device)?;
        #[cfg(test)]
        {
            self.pipeline_attempts += 1;
        }
        let result = create_quad_pipeline(device, key, &texture_layout, &material_layout);
        // Device loss is a namespace failure even if it also invalidated a shader handle.
        check_device()?;
        match result {
            Ok(pipeline) => {
                self.pipelines.push(CachedQuadPipeline::Ready(pipeline));
                Ok(())
            }
            Err(WgpuRenderError::ContentRejected { message }) => {
                self.pipelines.push(CachedQuadPipeline::Rejected {
                    key,
                    message: message.clone(),
                });
                Err(WgpuRenderError::ContentRejected { message })
            }
            Err(error) => Err(error),
        }
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
                    material: key.material,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::WgpuDeviceOwner;
    use crate::quad::{
        WgpuMaterial2d, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture, WgpuQuadTextureKey,
        draw_quad_batch_buffers,
    };
    use crate::readback_tests::{
        image_versions, readback_pixel, readback_viewport, render_to_rgba,
    };
    use nara_material::{AlphaMode2d, Material2dDefinition, Material2dInput, SamplerDescriptor};

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
        let [image, _] = image_versions(1, 1);
        let mut batch = cost_test_batch();
        let primary = StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap();
        batch.material_extension = Some(WgpuMaterial2d {
            definition: Material2dDefinition {
                id: "test.stock-preparation-cost",
                fragment_shader: "",
                input: Material2dInput::SPRITE_FRAGMENT_V1,
                alpha_mode: AlphaMode2d::Opaque,
            },
            parameters: [0.0; 4],
            texture: image,
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
        let bad_material = Material2dDefinition {
            id: "test.pipeline-rejection",
            fragment_shader: "this is not valid WGSL",
            input: Material2dInput::SPRITE_FRAGMENT_V1,
            alpha_mode: AlphaMode2d::Opaque,
        };
        let good_material = Material2dDefinition {
            fragment_shader: "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }",
            ..bad_material
        };
        let (pixels, (mut resources, mut owner, bad_key)) =
            render_to_rgba(|device, queue, format, output_view, encoder, preparation| {
                let mut owner = WgpuDeviceOwner::default();
                owner.install_probe_device(device.clone(), queue.clone());
                let mut resources = StockQuadResources::default();
                let bad_key = WgpuQuadPipelineKey {
                    format,
                    alpha_mode: AlphaMode2d::Opaque,
                    material: Some(bad_material),
                };
                for _ in 1..=4 {
                    resources.reset_instance_budget();
                    assert!(matches!(
                        resources
                            .ensure_pipeline(device, &|| owner.fail_if_device_lost(), bad_key,),
                        Err(WgpuRenderError::ContentRejected { .. })
                    ));
                }
                assert_eq!(resources.pipeline_attempts, 1);
                assert_eq!(resources.pipelines.len(), 1);
                let good_key = WgpuQuadPipelineKey {
                    material: Some(good_material),
                    ..bad_key
                };
                for _ in 5..=8 {
                    resources.reset_instance_budget();
                    resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), good_key)?;
                }
                assert_eq!(resources.pipeline_attempts, 2);
                let [image, _] = image_versions(1, 1);
                let prepared = resources.prepare(
                    preparation,
                    || owner.fail_if_device_lost(),
                    format,
                    WgpuLoweredFrameDraw {
                        batches: vec![WgpuQuadBatch {
                            material: WgpuQuadMaterialKey {
                                texture: WgpuQuadTextureKey::Fallback,
                                sampler: SamplerDescriptor::NEAREST_CLAMP,
                                alpha_mode: AlphaMode2d::Opaque,
                            },
                            texture: WgpuQuadTexture::Fallback,
                            scissor: None,
                            instances: vec![WgpuQuadInstance {
                                center: [0.0; 2],
                                x_axis: [1.0, 0.0],
                                y_axis: [0.0, 1.0],
                                color: [1.0; 4],
                                uv_min: [0.0; 2],
                                uv_size: [1.0; 2],
                                texture_mode: 0,
                                shape_mode: 0,
                                rect_size: [1.0; 2],
                                corner_radius: 0.0,
                                border_width: 0.0,
                            }],
                            counts_as_sprites: true,
                            material_extension: Some(WgpuMaterial2d {
                                definition: good_material,
                                parameters: [0.0; 4],
                                texture: image,
                            }),
                        }],
                        #[cfg(feature = "ui-submitter")]
                        ui_packets: Vec::new(),
                    },
                    8,
                )?;
                assert_eq!(resources.pipeline_attempts, 2);
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
                Ok((resources, owner, bad_key))
            })
            .unwrap();
        assert_eq!(readback_pixel(&pixels, 32, 32), &[0, 255, 0, 255]);
        resources.clear();
        owner.clear();
        assert!(resources.pipelines.is_empty());
        render_to_rgba(|device, queue, _, _, _, _| {
            owner.install_probe_device(device.clone(), queue.clone());
            for _ in 0..3 {
                assert!(matches!(
                    resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), bad_key,),
                    Err(WgpuRenderError::ContentRejected { .. })
                ));
            }
            assert_eq!(
                resources.pipeline_attempts, 3,
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
                    bad_key,
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
            let key = WgpuQuadPipelineKey {
                format,
                alpha_mode: AlphaMode2d::Opaque,
                material: Some(Material2dDefinition {
                    id: "test.pipeline-allocation",
                    fragment_shader: "invalid WGSL",
                    input: Material2dInput::SPRITE_FRAGMENT_V1,
                    alpha_mode: AlphaMode2d::Opaque,
                }),
            };
            // Allocation failure takes priority over WGSL rejection and remains retryable.
            QUAD_PIPELINE_ALLOCATION_FAILURE.with(|failure| failure.set(true));
            assert!(matches!(
                resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), key,),
                Err(WgpuRenderError::UploadDeferred { .. })
            ));
            assert!(resources.pipelines.is_empty());
            assert_eq!(resources.pipeline_attempts, 1);
            assert!(matches!(
                resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), key,),
                Err(WgpuRenderError::ContentRejected { .. })
            ));
            assert_eq!(resources.pipeline_attempts, 2);
            owner.mark_probe_lost();
            assert_eq!(
                resources.ensure_pipeline(device, &|| owner.fail_if_device_lost(), key,),
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
