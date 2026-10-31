//! The Host's final consumer of an already resolved, physical-resolution program image.

use std::sync::Arc;

use nara_render::Extent2d;

use crate::{WgpuRenderError, allocation::checked_resource as checked};

/// Caches the pipeline and layout for the active device epoch and surface format.
#[derive(Debug, Default)]
pub(crate) struct SurfacePresentationCache {
    pipeline: Option<Arc<SurfacePresentationPipeline>>,
}

#[derive(Debug)]
struct SurfacePresentationPipeline {
    device_epoch: u64,
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

/// Immutable final draw, retained by the Host in one pre-admitted custody entry.
///
/// The Host must also retain the exact program output storage and its byte guard through
/// the original submission prefix. A cloned view does not replace that storage custody.
#[derive(Debug)]
pub(crate) struct PreparedSurfacePresentation {
    pipeline: Arc<SurfacePresentationPipeline>,
    binding: wgpu::BindGroup,
    _source: wgpu::TextureView,
}

impl SurfacePresentationCache {
    pub(crate) fn modeled_metadata_bytes() -> usize {
        std::mem::size_of::<Self>()
            + std::mem::size_of::<SurfacePresentationPipeline>()
            + 2 * std::mem::size_of::<usize>()
            + crate::submission::WgpuSubmissionReservation::modeled_retain_bytes::<
                PreparedSurfacePresentation,
            >()
    }
    /// Releases cached handles without invalidating any already prepared draw.
    pub(crate) fn clear(&mut self) {
        self.pipeline = None;
    }

    /// Binds the exact final image using the Host's original enabled device.
    ///
    /// `source` must be a single-sample 2D view with `TEXTURE_BINDING` usage, in the
    /// same native format and transfer as the surface. Its contents already include all
    /// composition at the destination's physical resolution. The Host owns validation
    /// of the actual source descriptor, target descriptor and device namespace.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        device_epoch: u64,
        format: wgpu::TextureFormat,
        source: &wgpu::TextureView,
        source_extent: Extent2d,
        destination_extent: Extent2d,
    ) -> Result<PreparedSurfacePresentation, WgpuRenderError> {
        // Refuse scaling before either pipeline creation or binding the source.
        validate_extents(source_extent, destination_extent)?;
        let pipeline =
            match self.pipeline.as_ref().filter(|pipeline| {
                pipeline.device_epoch == device_epoch && pipeline.format == format
            }) {
                Some(pipeline) => Arc::clone(pipeline),
                None => Arc::new(SurfacePresentationPipeline::create(
                    device,
                    device_epoch,
                    format,
                )?),
            };
        let binding = checked(device, "nara_surface_presentation_binding", || {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nara_surface_presentation_binding"),
                layout: &pipeline.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                }],
            })
        })?;
        // A failed preparation cannot replace a successfully cached pipeline.
        self.pipeline = Some(Arc::clone(&pipeline));
        Ok(PreparedSurfacePresentation {
            pipeline,
            binding,
            _source: source.clone(),
        })
    }
}

fn validate_extents(source: Extent2d, destination: Extent2d) -> Result<(), WgpuRenderError> {
    if source != destination || source.is_empty() {
        return Err(WgpuRenderError::ContentRejected {
            message: format!(
                "surface presentation requires equal nonempty physical extents: source {source:?}, destination {destination:?}"
            ),
        });
    }
    Ok(())
}

impl SurfacePresentationPipeline {
    fn create(
        device: &wgpu::Device,
        device_epoch: u64,
        format: wgpu::TextureFormat,
    ) -> Result<Self, WgpuRenderError> {
        let layout = checked(device, "nara_surface_presentation_layout", || {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("nara_surface_presentation_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            })
        })?;
        let shader = checked(device, "nara_surface_presentation_shader", || {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("nara_surface_presentation_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("presentation.wgsl").into()),
            })
        })?;
        let pipeline_layout = checked(device, "nara_surface_presentation_pipeline_layout", || {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("nara_surface_presentation_pipeline_layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            })
        })?;
        let pipeline = checked(device, "nara_surface_presentation_pipeline", || {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("nara_surface_presentation_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })?;
        Ok(Self {
            device_epoch,
            format,
            layout,
            pipeline,
        })
    }
}

impl PreparedSurfacePresentation {
    /// Encodes into the Host's existing target transaction; the Host submits and presents.
    pub(crate) fn encode(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara_surface_presentation"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(&self.pipeline.pipeline);
        pass.set_bind_group(0, &self.binding, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests;
