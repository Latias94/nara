//! Independently authored native Features. The production Host owns all graph execution.

use std::sync::Arc;

use nara_render_wgpu::{
    WgpuFeatureAllocation, WgpuFeatureEncodeContext, WgpuFeaturePrepareContext, WgpuProgramBuffer,
    WgpuProgramTexture, WgpuRenderFeature, WgpuRenderProgramError,
};

use super::model::*;

pub struct Upload;

pub struct UploadCapture {
    pub destination: WgpuProgramTexture<SceneColor>,
    pub pixels: Arc<[[f32; 4]]>,
}

pub struct Uploaded {
    destination: WgpuProgramTexture<SceneColor>,
    staging: wgpu::Texture,
    _allocation: WgpuFeatureAllocation,
}

impl WgpuRenderFeature for Upload {
    type Capture = UploadCapture;
    type State = ();
    type Prepared = Uploaded;
    type Report = ();

    fn create_state(
        &self,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn prepare(
        &self,
        _state: &mut (),
        capture: &UploadCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Uploaded, WgpuRenderProgramError> {
        if capture.pixels.len() != (WIDTH * HEIGHT) as usize {
            return Err(WgpuRenderProgramError::feature(
                "expected exactly 64 by 64 RGBA32Float pixels",
            ));
        }
        let allocation = context.reserve_bytes(IMAGE_BYTES)?;
        let staging = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("public-program-upload-private-input"),
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            ..texture_descriptor().native
        });
        let bytes: Vec<u8> = capture
            .pixels
            .iter()
            .flatten()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        context.upload_texture(
            staging.as_image_copy(),
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 16),
                rows_per_image: Some(HEIGHT),
            },
            extent(),
        )?;
        Ok(Uploaded {
            destination: capture.destination.clone(),
            staging,
            _allocation: allocation,
        })
    }

    fn encode(
        &self,
        prepared: &Uploaded,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let destination = resources.texture_native(&prepared.destination)?;
        encoder.copy_texture_to_texture(
            prepared.staging.as_image_copy(),
            destination.as_image_copy(),
            extent(),
        );
        Ok(())
    }
}

pub struct ComputeState {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

pub struct ComputePrepared {
    pipeline: wgpu::ComputePipeline,
    group: wgpu::BindGroup,
}

fn compute_state(
    device: &wgpu::Device,
    label: &'static str,
    shader: &'static str,
    entry: &'static str,
    entries: &[wgpu::BindGroupLayoutEntry],
) -> ComputeState {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some(entry),
        compilation_options: Default::default(),
        cache: None,
    });
    ComputeState { pipeline, layout }
}

fn sampled(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn storage_texture(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba32Float,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}

fn storage_buffer(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(COUNTS_BYTES),
        },
        count: None,
    }
}

fn image_bindings<I: 'static, O: 'static>(
    state: &ComputeState,
    context: &WgpuFeaturePrepareContext<'_>,
    source: &WgpuProgramTexture<I>,
    destination: &WgpuProgramTexture<O>,
) -> Result<ComputePrepared, WgpuRenderProgramError> {
    let resources = context.resources();
    let group = context
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("public-program-image-bindings"),
            layout: &state.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(resources.texture(source)?),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(resources.texture(destination)?),
                },
            ],
        });
    Ok(ComputePrepared {
        pipeline: state.pipeline.clone(),
        group,
    })
}

fn dispatch(
    prepared: &ComputePrepared,
    context: &mut WgpuFeatureEncodeContext<'_>,
    x: u32,
    y: u32,
) {
    let mut pass = context
        .encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("public-program-compute"),
            ..Default::default()
        });
    pass.set_pipeline(&prepared.pipeline);
    pass.set_bind_group(0, &prepared.group, &[]);
    pass.dispatch_workgroups(x, y, 1);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

pub struct Blur(pub Axis);

pub struct BlurCapture {
    pub source: WgpuProgramTexture<SceneColor>,
    pub destination: WgpuProgramTexture<SceneColor>,
}

impl WgpuRenderFeature for Blur {
    type Capture = BlurCapture;
    type State = ComputeState;
    type Prepared = ComputePrepared;
    type Report = Axis;

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ComputeState, WgpuRenderProgramError> {
        let entry = match self.0 {
            Axis::Horizontal => "horizontal",
            Axis::Vertical => "vertical",
        };
        Ok(compute_state(
            context.device(),
            "public-program-blur",
            include_str!("shaders/blur.wgsl"),
            entry,
            &[sampled(0), storage_texture(1)],
        ))
    }

    fn prepare(
        &self,
        state: &mut ComputeState,
        capture: &BlurCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ComputePrepared, WgpuRenderProgramError> {
        image_bindings(state, context, &capture.source, &capture.destination)
    }

    fn encode(
        &self,
        prepared: &ComputePrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Axis, WgpuRenderProgramError> {
        dispatch(prepared, context, WIDTH.div_ceil(8), HEIGHT.div_ceil(8));
        Ok(self.0)
    }
}

// A separate Feature implementation, capture type, definition and native pipeline.
pub struct Complement;

pub struct ComplementCapture {
    pub source: WgpuProgramTexture<SceneColor>,
    pub destination: WgpuProgramTexture<EffectColor>,
}

impl WgpuRenderFeature for Complement {
    type Capture = ComplementCapture;
    type State = ComputeState;
    type Prepared = ComputePrepared;
    type Report = ();

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ComputeState, WgpuRenderProgramError> {
        Ok(compute_state(
            context.device(),
            "public-program-complement",
            include_str!("shaders/complement.wgsl"),
            "complement",
            &[sampled(0), storage_texture(1)],
        ))
    }

    fn prepare(
        &self,
        state: &mut ComputeState,
        capture: &ComplementCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ComputePrepared, WgpuRenderProgramError> {
        image_bindings(state, context, &capture.source, &capture.destination)
    }

    fn encode(
        &self,
        prepared: &ComputePrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        dispatch(prepared, context, WIDTH.div_ceil(8), HEIGHT.div_ceil(8));
        Ok(())
    }
}

pub struct Luminance;

pub struct LuminanceCapture {
    pub source: WgpuProgramTexture<SceneColor>,
    pub counts: WgpuProgramBuffer<RawCounts>,
}

impl WgpuRenderFeature for Luminance {
    type Capture = LuminanceCapture;
    type State = ComputeState;
    type Prepared = ComputePrepared;
    type Report = ();

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ComputeState, WgpuRenderProgramError> {
        Ok(compute_state(
            context.device(),
            "public-program-luminance",
            include_str!("shaders/luminance.wgsl"),
            "luminance",
            &[sampled(0), storage_buffer(1)],
        ))
    }

    fn prepare(
        &self,
        state: &mut ComputeState,
        capture: &LuminanceCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ComputePrepared, WgpuRenderProgramError> {
        let resources = context.resources();
        let group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("public-program-luminance-bindings"),
                layout: &state.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            resources.texture(&capture.source)?,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: resources.buffer(&capture.counts)?.as_entire_binding(),
                    },
                ],
            });
        Ok(ComputePrepared {
            pipeline: state.pipeline.clone(),
            group,
        })
    }

    fn encode(
        &self,
        prepared: &ComputePrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        dispatch(prepared, context, WIDTH.div_ceil(8), HEIGHT.div_ceil(8));
        Ok(())
    }
}

pub struct Normalize;

pub struct NormalizeCapture {
    pub frequencies: WgpuProgramBuffer<Frequencies>,
}

impl WgpuRenderFeature for Normalize {
    type Capture = NormalizeCapture;
    type State = ComputeState;
    type Prepared = ComputePrepared;
    type Report = ();

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ComputeState, WgpuRenderProgramError> {
        Ok(compute_state(
            context.device(),
            "public-program-normalize",
            include_str!("shaders/normalize.wgsl"),
            "normalize",
            &[storage_buffer(0)],
        ))
    }

    fn prepare(
        &self,
        state: &mut ComputeState,
        capture: &NormalizeCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ComputePrepared, WgpuRenderProgramError> {
        let group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("public-program-normalize-bindings"),
                layout: &state.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: context
                        .resources()
                        .buffer(&capture.frequencies)?
                        .as_entire_binding(),
                }],
            });
        Ok(ComputePrepared {
            pipeline: state.pipeline.clone(),
            group,
        })
    }

    fn encode(
        &self,
        prepared: &ComputePrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        dispatch(prepared, context, 1, 1);
        Ok(())
    }
}

pub struct Histogram;

pub struct HistogramCapture {
    pub frequencies: WgpuProgramBuffer<Frequencies>,
    pub destination: WgpuProgramTexture<HistogramColor>,
}

pub struct HistogramPrepared {
    pipeline: wgpu::RenderPipeline,
    capture: HistogramCapture,
}

impl WgpuRenderFeature for Histogram {
    type Capture = HistogramCapture;
    type State = wgpu::RenderPipeline;
    type Prepared = HistogramPrepared;
    type Report = ();

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::State, WgpuRenderProgramError> {
        let device = context.device();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("public-program-histogram"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/histogram.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("public-program-histogram"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        Ok(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("public-program-histogram"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 4,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 0,
                            shader_location: 0,
                        }],
                    })],
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
            }),
        )
    }

    fn prepare(
        &self,
        state: &mut Self::State,
        capture: &HistogramCapture,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<HistogramPrepared, WgpuRenderProgramError> {
        Ok(HistogramPrepared {
            pipeline: state.clone(),
            capture: HistogramCapture {
                frequencies: capture.frequencies.clone(),
                destination: capture.destination.clone(),
            },
        })
    }

    fn encode(
        &self,
        prepared: &HistogramPrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let destination = resources.texture(&prepared.capture.destination)?;
        let frequencies = resources.buffer(&prepared.capture.frequencies)?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("public-program-histogram-vertex-draw"),
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
        pass.set_vertex_buffer(0, frequencies.slice(..));
        pass.draw(0..6, 0..4);
        Ok(())
    }
}
