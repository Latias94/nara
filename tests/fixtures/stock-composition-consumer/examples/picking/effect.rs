use std::sync::Arc;

use nara::{
    core::Mat3,
    render::{Extent2d, RenderFramePacket, WorldInputMapping2d},
};
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureInterpretation, TextureTransfer,
    WgpuDeviceRequirements, WgpuFeatureAllocation, WgpuFeatureDeclaration, WgpuFeatureDefinition,
    WgpuFeatureEncodeContext, WgpuFeatureInstance, WgpuFeaturePreflightContext,
    WgpuFeaturePrepareContext, WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyProgram,
    WgpuProgramTexture, WgpuRenderFeature, WgpuRenderPolicy, WgpuRenderProgramError,
    WgpuSceneColor, WgpuStockBindings, WgpuStockCapture, WgpuStockStages,
};

use super::scene::SceneState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Identity,
    Tint,
    MirrorMapped,
    MirrorUnmapped,
}

pub struct PolicyCapture {
    pub stock: Arc<WgpuStockCapture>,
    pub mode: Mode,
    pub state: SceneState,
    pub extent: Extent2d,
}

pub struct PickingPolicy {
    stock: WgpuStockStages,
    effect: WgpuFeatureDeclaration<PixelEffect>,
}

impl PickingPolicy {
    pub fn new() -> Self {
        Self {
            stock: WgpuStockStages::new(),
            effect: WgpuFeatureDefinition::new(PixelEffect).declare(),
        }
    }
}

pub struct PickingBindings {
    stock: WgpuStockBindings,
    effect: Arc<WgpuFeatureInstance<PixelEffect>>,
}

impl WgpuRenderPolicy for PickingPolicy {
    type Capture = PolicyCapture;
    type Bindings = PickingBindings;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.stock
            .device_requirements()
            .union(self.effect.device_requirements())
    }

    fn world_input_mapping(&self, capture: &PolicyCapture) -> Option<WorldInputMapping2d> {
        match capture.mode {
            Mode::Identity | Mode::Tint => Some(WorldInputMapping2d::identity()),
            Mode::MirrorMapped => WorldInputMapping2d::affine(Mat3::from_cols_array(&[
                -1.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                capture.extent.width as f32,
                0.0,
                1.0,
            ])),
            Mode::MirrorUnmapped => None,
        }
    }

    fn bind(
        &self,
        _: &PolicyCapture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<PickingBindings, WgpuRenderProgramError> {
        Ok(PickingBindings {
            stock: self.stock.bind(context)?,
            effect: context.feature(&self.effect)?,
        })
    }

    fn validate_capture(
        &self,
        packet: &RenderFramePacket,
        capture: &PolicyCapture,
    ) -> Result<(), WgpuRenderProgramError> {
        if packet.target().extent() != capture.extent || packet.world_canvas().is_none() {
            return Err(WgpuRenderProgramError::feature(
                "picking policy requires the original final extent and fixed world canvas",
            ));
        }
        WgpuStockStages::validate_capture(packet, &capture.stock)
    }

    fn build(
        &self,
        bindings: &PickingBindings,
        capture: &PolicyCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut program = self.stock.begin(
            &bindings.stock,
            &capture.stock,
            context,
            GraphLimits::default(),
        )?;
        let world = program.world(10)?;
        let scene = program.resolve_canvas(20, &world)?;
        if capture.mode == Mode::Identity {
            return program.finish(40, &scene, &[world.output(), scene.output()]);
        }
        let result = program
            .graph()
            .texture::<WgpuSceneColor>(scene.descriptor().clone())?;
        program.graph().operation(
            30,
            &bindings.effect,
            Arc::new(PixelCapture {
                source: scene.clone(),
                destination: result.clone(),
                mode: capture.mode,
            }),
            &[scene.read(AccessRole::Sampled)],
            &[result.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
        )?;
        program.finish(
            40,
            &result,
            &[world.output(), scene.output(), result.output()],
        )
    }
}

struct PixelEffect;

struct PixelCapture {
    source: WgpuProgramTexture<WgpuSceneColor>,
    destination: WgpuProgramTexture<WgpuSceneColor>,
    mode: Mode,
}

impl PixelCapture {
    fn validate(&self) -> Result<(), WgpuRenderProgramError> {
        let source = self.source.descriptor();
        let destination = self.destination.descriptor();
        if source.native.size != destination.native.size
            || source.native.format != destination.native.format
            || !source
                .native
                .usage
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
            || !destination
                .native
                .usage
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(WgpuRenderProgramError::feature(
                "picking effect requires equal-extent, equal-format explicit sampled and attachment ports",
            ));
        }
        for descriptor in [source, destination] {
            let native = &descriptor.native;
            let transfer = if native.format.has_srgb_suffix() {
                TextureTransfer::Srgb
            } else {
                TextureTransfer::Linear
            };
            if !matches!(
                native.format,
                wgpu::TextureFormat::Rgba8Unorm
                    | wgpu::TextureFormat::Rgba8UnormSrgb
                    | wgpu::TextureFormat::Bgra8Unorm
                    | wgpu::TextureFormat::Bgra8UnormSrgb
            ) || descriptor.interpretation
                != (TextureInterpretation::Color {
                    transfer,
                    alpha: TextureAlpha::Premultiplied,
                })
                || native.dimension != wgpu::TextureDimension::D2
                || native.size.width == 0
                || native.size.height == 0
                || native.size.depth_or_array_layers != 1
                || native.sample_count != 1
                || native.mip_level_count != 1
                || !native.view_formats.is_empty()
            {
                return Err(WgpuRenderProgramError::feature(
                    "picking effect requires whole single-sample RGBA8/BGRA8 premultiplied color ports",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct PixelState {
    pipeline: Option<PixelPipeline>,
}

#[derive(Clone)]
struct PixelPipeline {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl PixelPipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("picking-effect-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("picking-effect-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("picking-effect-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("effect.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("picking-effect-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
        Self {
            format,
            layout,
            pipeline,
        }
    }
}

struct PixelPrepared {
    destination: WgpuProgramTexture<WgpuSceneColor>,
    pipeline: PixelPipeline,
    bindings: wgpu::BindGroup,
    _uniform: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

impl WgpuRenderFeature for PixelEffect {
    type Capture = PixelCapture;
    type State = PixelState;
    type Prepared = PixelPrepared;
    type Report = ();

    fn preflight(
        &self,
        capture: &PixelCapture,
        _: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        capture.validate()
    }

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<PixelState, WgpuRenderProgramError> {
        Ok(PixelState::default())
    }

    fn prepare(
        &self,
        state: &mut PixelState,
        capture: &PixelCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<PixelPrepared, WgpuRenderProgramError> {
        let resources = context.resources();
        let source = resources.texture(&capture.source)?;
        resources.texture(&capture.destination)?;
        let allocation = context.reserve_bytes(16)?;
        let format = capture.destination.descriptor().native.format;
        let pipeline = match &state.pipeline {
            Some(pipeline) if pipeline.format == format => pipeline.clone(),
            _ => PixelPipeline::new(context.device(), format),
        };
        let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("picking-effect-captured-mode"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        let tag = match capture.mode {
            Mode::Identity => 0_u32,
            Mode::Tint => 1,
            Mode::MirrorMapped | Mode::MirrorUnmapped => 2,
        };
        let bytes = [tag, 0, 0, 0].map(u32::to_le_bytes);
        uniform
            .get_mapped_range_mut(..)
            .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?
            .copy_from_slice(bytes.as_flattened());
        uniform.unmap();
        let bindings = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("picking-effect-bindings"),
                layout: &pipeline.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            });
        state.pipeline = Some(pipeline.clone());
        Ok(PixelPrepared {
            destination: capture.destination.clone(),
            pipeline,
            bindings,
            _uniform: uniform,
            _allocation: allocation,
        })
    }

    fn encode(
        &self,
        prepared: &PixelPrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let destination = resources.texture(&prepared.destination)?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("picking-effect-pass"),
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
        pass.set_pipeline(&prepared.pipeline.pipeline);
        pass.set_bind_group(0, &prepared.bindings, &[]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}
