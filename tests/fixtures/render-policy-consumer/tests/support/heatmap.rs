//! A screen-space scalar domain, independently authored policy and native drawing Feature.

use std::{num::NonZeroU64, sync::Arc};

use nara_ecs::Resource;
use nara_render::{FrameStats, RenderFramePacket, ViewportRect};
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuDeviceRequirements,
    WgpuFeatureAllocation, WgpuFeatureDeclaration, WgpuFeatureDefinition, WgpuFeatureEncodeContext,
    WgpuFeatureInstance, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyFrameFacts, WgpuPolicyProgram,
    WgpuProgramTexture, WgpuProgramTextureDescriptor, WgpuRenderFeature, WgpuRenderPolicy,
    WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

pub const GRID_SIDE: u32 = 4;
const PARAMETER_BYTES: u64 = 80;

#[derive(Clone, Debug, Resource)]
pub struct ScalarField {
    pub samples: [u32; 16],
}

impl Default for ScalarField {
    fn default() -> Self {
        Self {
            samples: [0, 1, 2, 3, 1, 3, 0, 2, 3, 2, 1, 0, 2, 0, 3, 1],
        }
    }
}

impl ScalarField {
    pub fn capture(&self) -> Arc<HeatmapCapture> {
        Arc::new(HeatmapCapture {
            samples: self.samples,
        })
    }
}

#[derive(Clone, Debug)]
pub struct HeatmapCapture {
    pub samples: [u32; 16],
}

pub struct HeatmapPolicy {
    feature: WgpuFeatureDeclaration<HeatmapFeature>,
}

impl HeatmapPolicy {
    pub fn new(palette_shift: u32) -> Self {
        assert!(palette_shift < GRID_SIDE);
        Self {
            feature: WgpuFeatureDefinition::new(HeatmapFeature { palette_shift }).declare(),
        }
    }
}

pub struct HeatmapBindings {
    feature: Arc<WgpuFeatureInstance<HeatmapFeature>>,
}

pub struct HeatmapColor;

impl WgpuRenderPolicy for HeatmapPolicy {
    type Capture = HeatmapCapture;
    type Bindings = HeatmapBindings;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.feature.device_requirements().clone()
    }

    fn bind(
        &self,
        _: &HeatmapCapture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<HeatmapBindings, WgpuRenderProgramError> {
        Ok(HeatmapBindings {
            feature: context.feature(&self.feature)?,
        })
    }

    fn validate_capture(
        &self,
        packet: &RenderFramePacket,
        capture: &HeatmapCapture,
    ) -> Result<(), WgpuRenderProgramError> {
        validate_samples(capture)?;
        let extent = packet.target().extent();
        if packet.world_canvas().is_some()
            || packet.views().len() != 1
            || extent.width > 1024
            || extent.height > 1024
            || Some(packet.views()[0].viewport) != ViewportRect::from_extent(extent)
        {
            return Err(WgpuRenderProgramError::feature(
                "the heatmap requires one full-target View, no fixed canvas, and at most 1024x1024 pixels",
            ));
        }
        Ok(())
    }

    fn build(
        &self,
        bindings: &HeatmapBindings,
        capture: &HeatmapCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        self.validate_capture(context.packet(), capture)?;
        let packet = context.packet();
        let extent = packet.target().extent();
        let format = context.format();
        let mut builder = WgpuRenderProgramBuilder::new(
            packet.source_generation(),
            packet.frame_index(),
            GraphLimits {
                max_resources: 2,
                max_operations: 2,
                max_accesses: 4,
                max_outputs: 1,
                max_modeled_resource_bytes: 4 * 1024 * 1024,
                max_modeled_metadata_bytes: 4 * 1024 * 1024,
                ..GraphLimits::default()
            },
        )?;
        let target = builder.texture::<HeatmapColor>(WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("independent-policy-heatmap"),
                size: wgpu::Extent3d {
                    width: extent.width,
                    height: extent.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            transfer: if format.is_srgb() {
                TextureTransfer::Srgb
            } else {
                TextureTransfer::Linear
            },
            alpha: TextureAlpha::Premultiplied,
        })?;
        builder.operation(
            17,
            &bindings.feature,
            Arc::new(HeatmapDrawCapture {
                target: target.clone(),
                samples: capture.samples,
            }),
            &[],
            &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )?;
        WgpuPolicyProgram::new(
            builder.finish(&[target.output()])?,
            &target,
            WgpuPolicyFrameFacts {
                frame: FrameStats {
                    draw_calls: 1,
                    sprites: 0,
                },
                ..WgpuPolicyFrameFacts::default()
            },
        )
    }
}

fn validate_samples(capture: &HeatmapCapture) -> Result<(), WgpuRenderProgramError> {
    if capture.samples.iter().any(|sample| *sample >= GRID_SIDE) {
        return Err(WgpuRenderProgramError::feature(
            "the heatmap accepts sixteen scalar samples in 0..4",
        ));
    }
    Ok(())
}

pub struct HeatmapFeature {
    palette_shift: u32,
}

pub struct HeatmapDrawCapture {
    target: WgpuProgramTexture<HeatmapColor>,
    samples: [u32; 16],
}

pub struct HeatmapDraw {
    target: WgpuProgramTexture<HeatmapColor>,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    _uniform: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

impl WgpuRenderFeature for HeatmapFeature {
    type Capture = HeatmapDrawCapture;
    type State = Option<(
        wgpu::TextureFormat,
        wgpu::RenderPipeline,
        wgpu::BindGroupLayout,
    )>;
    type Prepared = HeatmapDraw;
    type Report = ();

    fn preflight(
        &self,
        capture: &HeatmapDrawCapture,
        _context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        validate_samples(&HeatmapCapture {
            samples: capture.samples,
        })
    }

    fn create_state(
        &self,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::State, WgpuRenderProgramError> {
        Ok(None)
    }

    fn prepare(
        &self,
        state: &mut Self::State,
        capture: &HeatmapDrawCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<HeatmapDraw, WgpuRenderProgramError> {
        let descriptor = &capture.target.descriptor().native;
        let format = descriptor.format;
        if state.as_ref().is_none_or(|cached| cached.0 != format) {
            let (pipeline, layout) = pipeline(context.device(), format);
            *state = Some((format, pipeline, layout));
        }
        let (_, pipeline, layout) = state.as_ref().expect("the actual target format is cached");
        let allocation = context.reserve_bytes(PARAMETER_BYTES)?;
        let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("independent-heatmap-scalar-snapshot"),
            size: PARAMETER_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bytes: Vec<u8> = [
            descriptor.size.width,
            descriptor.size.height,
            self.palette_shift,
            0,
        ]
        .into_iter()
        .chain(capture.samples)
        .flat_map(u32::to_le_bytes)
        .collect();
        context.upload_buffer(&uniform, 0, &bytes)?;
        let group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("independent-heatmap-parameters"),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
        Ok(HeatmapDraw {
            target: capture.target.clone(),
            pipeline: pipeline.clone(),
            group,
            _uniform: uniform,
            _allocation: allocation,
        })
    }

    fn encode(
        &self,
        prepared: &HeatmapDraw,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let destination = resources.texture(&prepared.target)?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("independent-heatmap-draw"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&prepared.pipeline);
        pass.set_bind_group(0, &prepared.group, &[]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}

fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("independent-heatmap-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(PARAMETER_BYTES),
            },
            count: None,
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("independent-heatmap-pipeline-layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("independent-heatmap-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("heatmap.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("independent-heatmap-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragment"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, layout)
}
