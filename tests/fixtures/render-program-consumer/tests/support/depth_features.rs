use std::sync::Arc;

use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    AccessRole, WgpuFeatureAllocation, WgpuFeatureDefinition, WgpuFeatureEncodeContext,
    WgpuFeatureInstance, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext, WgpuProgramOutput,
    WgpuProgramOutputs, WgpuProgramTexture, WgpuProgramTextureOutput, WgpuRenderFeature,
    WgpuRenderHost, WgpuRenderProducer, WgpuRenderProgram, WgpuRenderProgramBuilder,
    WgpuRenderProgramError,
};

use crate::depth_model::*;

pub struct Raster;

pub struct RasterCapture {
    pub scene: Arc<Scene>,
    pub color: WgpuProgramTexture<Color>,
    pub depth: WgpuProgramTexture<Depth>,
    pub scissor: [u32; 4],
}

pub struct RasterState {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

pub struct RasterPrepared {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    _uniform: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
    color: WgpuProgramTexture<Color>,
    depth: WgpuProgramTexture<Depth>,
    scissor: [u32; 4],
}

impl WgpuRenderFeature for Raster {
    type Capture = RasterCapture;
    type State = RasterState;
    type Prepared = RasterPrepared;
    type Report = ();

    fn preflight(
        &self,
        capture: &RasterCapture,
        _context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        capture.scene.validate()?;
        let [x, y, width, height] = capture.scissor;
        if width == 0
            || height == 0
            || x.checked_add(width).is_none_or(|end| end > WIDTH)
            || y.checked_add(height).is_none_or(|end| end > HEIGHT)
        {
            return Err(WgpuRenderProgramError::feature(
                "invalid bounded raster scissor",
            ));
        }
        Ok(())
    }

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<RasterState, WgpuRenderProgramError> {
        let device = context.device();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("independent-depth-camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTES),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("independent-depth-raster-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("independent-depth-world-geometry"),
            source: wgpu::ShaderSource::Wgsl(include_str!("depth_raster.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("independent-depth-raster"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 28,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
        Ok(RasterState { pipeline, layout })
    }

    fn prepare(
        &self,
        state: &mut RasterState,
        capture: &RasterCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<RasterPrepared, WgpuRenderProgramError> {
        let allocation = context.reserve_bytes(INPUT_BYTES)?;
        let vertices = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("independent-depth-authored-vertices"),
            size: VERTEX_BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("independent-depth-captured-camera-and-units"),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene = &capture.scene;
        let order = match scene.order {
            DrawOrder::NearFirst => [0, 1],
            DrawOrder::FarFirst => [1, 0],
        };
        let mut vertex_bytes = Vec::with_capacity(VERTEX_BYTES as usize);
        for index in order {
            let quad = &scene.quads[index];
            for corner in [0, 1, 2, 0, 2, 3] {
                for value in quad.positions[corner].into_iter().chain(quad.color) {
                    vertex_bytes.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        let camera = scene.camera;
        let near = camera.near_m as f32;
        let far = camera.far_m as f32;
        let sy = 1.0 / camera.tan_half_fov_y as f32;
        let sx = sy * HEIGHT as f32 / WIDTH as f32;
        let z_scale = far / (far - near);
        let projection_and_camera = [
            sx,
            0.0,
            0.0,
            0.0,
            0.0,
            sy,
            0.0,
            0.0,
            0.0,
            0.0,
            z_scale,
            1.0,
            0.0,
            0.0,
            -near * z_scale,
            0.0,
            camera.eye_m[0] as f32,
            camera.eye_m[1] as f32,
            camera.eye_m[2] as f32,
            0.0,
            scene.meters_per_unit as f32,
            0.0,
            0.0,
            0.0,
        ];
        let uniform_bytes: Vec<u8> = projection_and_camera
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        context.upload_buffer(&vertices, 0, &vertex_bytes)?;
        context.upload_buffer(&uniform, 0, &uniform_bytes)?;
        let group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("independent-depth-camera-binding"),
                layout: &state.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
        Ok(RasterPrepared {
            pipeline: state.pipeline.clone(),
            group,
            vertices,
            _uniform: uniform,
            _allocation: allocation,
            color: capture.color.clone(),
            depth: capture.depth.clone(),
            scissor: capture.scissor,
        })
    }

    fn encode(
        &self,
        prepared: &RasterPrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let color = resources.texture(&prepared.color)?;
        let depth = resources.texture(&prepared.depth)?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("independent-depth-load-store-subrect"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&prepared.pipeline);
        pass.set_bind_group(0, &prepared.group, &[]);
        pass.set_vertex_buffer(0, prepared.vertices.slice(..));
        let [x, y, width, height] = prepared.scissor;
        pass.set_scissor_rect(x, y, width, height);
        pass.draw(0..6, 0..1);
        pass.draw(6..12, 0..1);
        Ok(())
    }
}

pub struct Resolve;

#[derive(Clone)]
pub struct ResolveCapture {
    pub depth: WgpuProgramTexture<Depth>,
    pub destination: WgpuProgramTexture<ResolvedDepth>,
}

pub struct ResolveState {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

pub struct ResolvePrepared {
    pipeline: wgpu::ComputePipeline,
    group: wgpu::BindGroup,
}

impl WgpuRenderFeature for Resolve {
    type Capture = ResolveCapture;
    type State = ResolveState;
    type Prepared = ResolvePrepared;
    type Report = ();

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ResolveState, WgpuRenderProgramError> {
        let device = context.device();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("independent-depth-sampling-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("independent-depth-resolve-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("independent-depth-sampling"),
            source: wgpu::ShaderSource::Wgsl(include_str!("depth_resolve.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("independent-depth-resolve"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("resolve"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(ResolveState { pipeline, layout })
    }

    fn prepare(
        &self,
        state: &mut ResolveState,
        capture: &ResolveCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ResolvePrepared, WgpuRenderProgramError> {
        let resources = context.resources();
        let group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("independent-depth-declared-sample"),
                layout: &state.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            resources.texture(&capture.depth)?,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            resources.texture(&capture.destination)?,
                        ),
                    },
                ],
            });
        Ok(ResolvePrepared {
            pipeline: state.pipeline.clone(),
            group,
        })
    }

    fn encode(
        &self,
        prepared: &ResolvePrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let mut pass = context
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("independent-depth-resolve-dispatch"),
                ..Default::default()
            });
        pass.set_pipeline(&prepared.pipeline);
        pass.set_bind_group(0, &prepared.group, &[]);
        pass.dispatch_workgroups(WIDTH.div_ceil(8), HEIGHT.div_ceil(8), 1);
        Ok(())
    }
}

pub fn install<F: WgpuRenderFeature>(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    feature: F,
) -> WgpuFeatureInstance<F> {
    host.create_feature_instance(producer, &WgpuFeatureDefinition::new(feature))
        .expect("install the original external Feature")
}

pub struct Installed {
    pub raster: WgpuFeatureInstance<Raster>,
    pub resolve: WgpuFeatureInstance<Resolve>,
}

impl Installed {
    pub fn new(host: &mut WgpuRenderHost, producer: &WgpuRenderProducer) -> Self {
        Self {
            raster: install(host, producer, Raster),
            resolve: install(host, producer, Resolve),
        }
    }
}

#[derive(Clone)]
pub struct Ports {
    pub color: WgpuProgramTexture<Color>,
    pub depth: WgpuProgramTexture<Depth>,
    pub preserved_color: WgpuProgramTexture<Color>,
    pub preserved_depth: WgpuProgramTexture<Depth>,
    pub resolved: WgpuProgramTexture<ResolvedDepth>,
}

impl Ports {
    pub fn outputs(&self) -> [WgpuProgramOutput; 5] {
        [
            self.resolved.output(),
            self.depth.output(),
            self.preserved_color.output(),
            self.color.output(),
            self.preserved_depth.output(),
        ]
    }

    pub fn published(&self, outputs: &WgpuProgramOutputs) -> Published {
        Published {
            color: outputs
                .texture(&self.color)
                .expect("publish original color"),
            depth: outputs
                .texture(&self.depth)
                .expect("publish original Depth32Float"),
            preserved_color: outputs
                .texture(&self.preserved_color)
                .expect("publish preserved color"),
            preserved_depth: outputs
                .texture(&self.preserved_depth)
                .expect("publish preserved depth"),
            resolved: outputs
                .texture(&self.resolved)
                .expect("publish sampled preserved depth"),
        }
    }
}

pub struct Published {
    pub color: WgpuProgramTextureOutput<Color>,
    pub depth: WgpuProgramTextureOutput<Depth>,
    pub preserved_color: WgpuProgramTextureOutput<Color>,
    pub preserved_depth: WgpuProgramTextureOutput<Depth>,
    pub resolved: WgpuProgramTextureOutput<ResolvedDepth>,
}

impl Published {
    pub fn import(&self, builder: &mut WgpuRenderProgramBuilder) -> Ports {
        Ports {
            color: builder
                .import_texture(&self.color)
                .expect("import original color lease"),
            depth: builder
                .import_texture(&self.depth)
                .expect("import original depth lease"),
            preserved_color: builder
                .import_texture(&self.preserved_color)
                .expect("import preserved color lease"),
            preserved_depth: builder
                .import_texture(&self.preserved_depth)
                .expect("import preserved depth lease"),
            resolved: builder
                .import_texture(&self.resolved)
                .expect("import original resolve lease"),
        }
    }
}

pub struct Workload {
    pub program: WgpuRenderProgram,
    pub ports: Ports,
}

pub fn resolve(
    builder: &mut WgpuRenderProgramBuilder,
    key: u64,
    instance: &WgpuFeatureInstance<Resolve>,
    depth: &WgpuProgramTexture<Depth>,
    destination: &WgpuProgramTexture<ResolvedDepth>,
) {
    builder
        .operation(
            key,
            instance,
            Arc::new(ResolveCapture {
                depth: depth.clone(),
                destination: destination.clone(),
            }),
            &[depth.read(AccessRole::Sampled)],
            &[destination.clear(wgpu::Color::TRANSPARENT, AccessRole::StorageWrite)],
        )
        .expect("declare an ordinary depth sampling operation");
}

pub fn build(
    source: RenderFrameSourceGeneration,
    frame: u64,
    scene: Arc<Scene>,
    installed: &Installed,
) -> Workload {
    let mut builder = WgpuRenderProgramBuilder::new(source, frame, limits()).unwrap();
    let ports = Ports {
        color: builder.texture(color_descriptor()).unwrap(),
        depth: builder.texture(depth_descriptor()).unwrap(),
        preserved_color: builder.texture(color_descriptor()).unwrap(),
        preserved_depth: builder.texture(depth_descriptor()).unwrap(),
        resolved: builder.texture(color_descriptor()).unwrap(),
    };
    resolve(
        &mut builder,
        10,
        &installed.resolve,
        &ports.preserved_depth,
        &ports.resolved,
    );
    builder
        .operation(
            20,
            &installed.raster,
            Arc::new(RasterCapture {
                scene: Arc::new(scene.preserved()),
                color: ports.preserved_color.clone(),
                depth: ports.preserved_depth.clone(),
                scissor: PRESERVE_SCISSOR,
            }),
            &[],
            &[
                ports
                    .preserved_color
                    .preserve(&ports.color, AccessRole::Attachment),
                ports
                    .preserved_depth
                    .preserve(&ports.depth, AccessRole::Attachment),
            ],
        )
        .expect("load preserved color and depth in a smaller scissor");
    builder
        .operation(
            90,
            &installed.raster,
            Arc::new(RasterCapture {
                scene,
                color: ports.color.clone(),
                depth: ports.depth.clone(),
                scissor: RASTER_SCISSOR,
            }),
            &[],
            &[
                ports
                    .color
                    .clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment),
                ports.depth.clear_depth(1.0, AccessRole::Attachment),
            ],
        )
        .expect("clear complete attachments before the first restricted draw");
    let program = builder
        .finish(&ports.outputs())
        .expect("complete external 3D depth program");
    Workload { program, ports }
}
