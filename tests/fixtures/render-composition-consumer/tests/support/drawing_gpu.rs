//! Public typed Features only; the original Host owns device acquisition and every submission.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    AccessRole, WgpuFeatureAllocation, WgpuFeatureDefinition, WgpuFeatureEncodeContext,
    WgpuFeatureInstance, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuProgramTexture, WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderHost,
    WgpuRenderProducer, WgpuRenderProgram, WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

use super::drawing_model::*;

pub const PATTERN: u64 = 30;
pub const DRAWING: u64 = 20;
pub const EFFECT: u64 = 10;

pub fn install<F: WgpuRenderFeature>(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    feature: F,
) -> WgpuFeatureInstance<F> {
    host.create_feature_instance(producer, &WgpuFeatureDefinition::new(feature))
        .expect("install this producer's exact public Feature definition")
}

/// The report identifies the original typed graph port without exposing its native allocation.
pub struct WrittenTexture<S: 'static> {
    pub port: WgpuProgramTexture<S>,
}

#[derive(Clone)]
pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl Pipeline {
    fn new(device: &wgpu::Device, shader: &'static str, sampled: bool) -> Self {
        let mut entries = Vec::with_capacity(2);
        if sampled {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: u32::from(sampled),
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("finite-drawing-layout"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("finite-drawing-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("finite-drawing-shader"),
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("finite-drawing-pipeline"),
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
                    format: wgpu::TextureFormat::Rgba32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout }
    }

    fn bindings(
        &self,
        device: &wgpu::Device,
        source: Option<&wgpu::TextureView>,
        uniform: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let mut entries = Vec::with_capacity(2);
        if let Some(source) = source {
            entries.push(wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: u32::from(source.is_some()),
            resource: uniform.as_entire_binding(),
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("finite-drawing-exact-bindings"),
            layout: &self.layout,
            entries: &entries,
        })
    }
}

struct Uniform {
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

fn uniform(
    context: &mut WgpuFeaturePrepareContext<'_>,
    bytes: &[u8],
) -> Result<Uniform, WgpuRenderProgramError> {
    assert!(matches!(
        bytes.len() as u64,
        SMALL_UNIFORM_BYTES | DRAWING_UNIFORM_BYTES
    ));
    let allocation = context.reserve_bytes(bytes.len() as u64)?;
    let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("finite-drawing-captured-uniform"),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });
    buffer
        .get_mapped_range_mut(..)
        .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?
        .copy_from_slice(bytes);
    buffer.unmap();
    Ok(Uniform {
        buffer,
        _allocation: allocation,
    })
}

pub struct Prepared<S: 'static> {
    destination: WgpuProgramTexture<S>,
    pipeline: Pipeline,
    bindings: wgpu::BindGroup,
    _uniform: Uniform,
}

fn draw<S: 'static>(
    prepared: &Prepared<S>,
    context: &mut WgpuFeatureEncodeContext<'_>,
) -> Result<WrittenTexture<S>, WgpuRenderProgramError> {
    let (resources, encoder) = context.split();
    let target = resources.texture(&prepared.destination)?;
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("finite-drawing-full-initialized-attachment"),
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
        pass.set_pipeline(&prepared.pipeline.pipeline);
        pass.set_bind_group(0, &prepared.bindings, &[]);
        pass.draw(0..3, 0..1);
    }
    Ok(WrittenTexture {
        port: prepared.destination.clone(),
    })
}

fn shape<S: 'static>(port: &WgpuProgramTexture<S>, width: u32, height: u32) -> bool {
    let desc = port.descriptor();
    desc.native.size == descriptor(width, height).native.size
        && desc.native.format == wgpu::TextureFormat::Rgba32Float
        && desc.transfer == nara_render_wgpu::TextureTransfer::Linear
        && desc.alpha == nara_render_wgpu::TextureAlpha::Premultiplied
}

pub struct Pattern(pub Arc<AtomicUsize>);
pub struct PatternCapture {
    pub destination: WgpuProgramTexture<SceneColor>,
    pub phase: u32,
}

impl WgpuRenderFeature for Pattern {
    type Capture = PatternCapture;
    type State = Pipeline;
    type Prepared = Prepared<SceneColor>;
    type Report = WrittenTexture<SceneColor>;

    fn preflight(
        &self,
        capture: &PatternCapture,
        _: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        if capture.phase > 15 || !shape(&capture.destination, SCENE_WIDTH, SCENE_HEIGHT) {
            return Err(WgpuRenderProgramError::feature(
                "expected a finite 8x6 Scene pattern",
            ));
        }
        Ok(())
    }

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Pipeline, WgpuRenderProgramError> {
        let adapter = context.device().adapter_info();
        if adapter.backend != wgpu::Backend::Dx12
            || !matches!(
                adapter.device_type,
                wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu
            )
        {
            return Err(WgpuRenderProgramError::feature(
                "the numerical tracer requires actual DX12 hardware",
            ));
        }
        eprintln!(
            "finite drawing adapter: name={}, backend={:?}, type={:?}, driver={}, driver_info={}",
            adapter.name, adapter.backend, adapter.device_type, adapter.driver, adapter.driver_info
        );
        Ok(Pipeline::new(
            context.device(),
            include_str!("drawing_pattern.wgsl"),
            false,
        ))
    }

    fn prepare(
        &self,
        state: &mut Pipeline,
        capture: &PatternCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let bytes: Vec<_> = [capture.phase, 0, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let uniform = uniform(context, &bytes)?;
        let bindings = state.bindings(context.device(), None, &uniform.buffer);
        Ok(Prepared {
            destination: capture.destination.clone(),
            pipeline: state.clone(),
            bindings,
            _uniform: uniform,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        let report = draw(prepared, context)?;
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(report)
    }
}

pub struct Drawing;
pub struct DrawingCapture {
    pub source: WgpuProgramTexture<SceneColor>,
    pub destination: WgpuProgramTexture<DrawingColor>,
    pub placement: Placement,
}

impl WgpuRenderFeature for Drawing {
    type Capture = DrawingCapture;
    type State = Pipeline;
    type Prepared = Prepared<DrawingColor>;
    type Report = WrittenTexture<DrawingColor>;

    fn preflight(
        &self,
        capture: &DrawingCapture,
        _: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        if !capture.placement.valid()
            || !shape(&capture.source, SCENE_WIDTH, SCENE_HEIGHT)
            || !shape(&capture.destination, WIDTH, HEIGHT)
        {
            return Err(WgpuRenderProgramError::feature(
                "invalid finite captured drawing placement",
            ));
        }
        Ok(())
    }

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Pipeline, WgpuRenderProgramError> {
        Ok(Pipeline::new(
            context.device(),
            include_str!("drawing_composition.wgsl"),
            true,
        ))
    }

    fn prepare(
        &self,
        state: &mut Pipeline,
        capture: &DrawingCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let uniform = uniform(context, &capture.placement.uniform_bytes())?;
        let bindings = state.bindings(
            context.device(),
            Some(context.resources().texture(&capture.source)?),
            &uniform.buffer,
        );
        Ok(Prepared {
            destination: capture.destination.clone(),
            pipeline: state.clone(),
            bindings,
            _uniform: uniform,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        draw(prepared, context)
    }
}

pub struct SecondEffect(pub Arc<AtomicUsize>);
pub struct EffectCapture {
    pub source: WgpuProgramTexture<DrawingColor>,
    pub destination: WgpuProgramTexture<EffectColor>,
    pub gain: f32,
}

impl WgpuRenderFeature for SecondEffect {
    type Capture = EffectCapture;
    type State = Pipeline;
    type Prepared = Prepared<EffectColor>;
    type Report = WrittenTexture<EffectColor>;

    fn preflight(
        &self,
        capture: &EffectCapture,
        _: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        if !capture.gain.is_finite()
            || !(0.0..=1.0).contains(&capture.gain)
            || !shape(&capture.source, WIDTH, HEIGHT)
            || !shape(&capture.destination, WIDTH, HEIGHT)
        {
            return Err(WgpuRenderProgramError::feature(
                "invalid finite second effect",
            ));
        }
        Ok(())
    }

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Pipeline, WgpuRenderProgramError> {
        Ok(Pipeline::new(
            context.device(),
            include_str!("drawing_effect.wgsl"),
            true,
        ))
    }

    fn prepare(
        &self,
        state: &mut Pipeline,
        capture: &EffectCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let bytes: Vec<_> = [capture.gain, 0.0, 0.0, 0.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let uniform = uniform(context, &bytes)?;
        let bindings = state.bindings(
            context.device(),
            Some(context.resources().texture(&capture.source)?),
            &uniform.buffer,
        );
        Ok(Prepared {
            destination: capture.destination.clone(),
            pipeline: state.clone(),
            bindings,
            _uniform: uniform,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        draw(prepared, context)
    }
}

pub fn pattern(
    source: RenderFrameSourceGeneration,
    frame: u64,
    phase: u32,
    instance: &WgpuFeatureInstance<Pattern>,
) -> WgpuRenderProgram {
    let mut builder = WgpuRenderProgramBuilder::new(source, frame, limits()).unwrap();
    let destination = builder
        .texture::<SceneColor>(descriptor(SCENE_WIDTH, SCENE_HEIGHT))
        .unwrap();
    builder
        .operation(
            PATTERN,
            instance,
            Arc::new(PatternCapture {
                destination: destination.clone(),
                phase,
            }),
            &[],
            &[destination.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
        )
        .unwrap();
    builder.finish(&[destination.output()]).unwrap()
}

/// Receipt and placement are captured together. Later layout mutation cannot retarget either.
#[derive(Clone)]
pub struct CapturedDrawing {
    pub scene: WgpuProgramTextureOutput<SceneColor>,
    pub placement: Placement,
}

pub fn compose(
    source: RenderFrameSourceGeneration,
    frame: u64,
    captured: &CapturedDrawing,
    drawing: &WgpuFeatureInstance<Drawing>,
    effect: Option<&WgpuFeatureInstance<SecondEffect>>,
) -> WgpuRenderProgram {
    let mut builder = WgpuRenderProgramBuilder::new(source, frame, limits()).unwrap();
    let scene = builder.import_texture(&captured.scene).unwrap();
    let target = builder
        .texture::<DrawingColor>(descriptor(WIDTH, HEIGHT))
        .unwrap();
    // Reverse registration and operation keys make the actual sampled dependency necessary.
    let final_target = effect.map(|instance| {
        let final_target = builder
            .texture::<EffectColor>(descriptor(WIDTH, HEIGHT))
            .unwrap();
        builder
            .operation(
                EFFECT,
                instance,
                Arc::new(EffectCapture {
                    source: target.clone(),
                    destination: final_target.clone(),
                    gain: 0.5,
                }),
                &[target.read(AccessRole::Sampled)],
                &[final_target.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        final_target
    });
    builder
        .operation(
            DRAWING,
            drawing,
            Arc::new(DrawingCapture {
                source: scene.clone(),
                destination: target.clone(),
                placement: captured.placement,
            }),
            &[scene.read(AccessRole::Sampled)],
            &[target.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
        )
        .unwrap();
    let mut outputs = vec![target.output()];
    if let Some(final_target) = final_target {
        outputs.insert(0, final_target.output());
    }
    builder.finish(&outputs).unwrap()
}
