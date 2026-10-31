//! Concrete exact-wgpu kernels shared by the private graph and ordered/slot controls.
//!
//! The caller owns every texture and buffer, initialization, copies and submission. Image
//! effects require distinct, equally sized, complete single-mip/single-layer RGBA32Float views.
//! Colors are finite linear premultiplied RGBA. No effect allocates a texture or buffer.

pub(super) const HISTOGRAM_BINS: u32 = 4;
pub(super) const COUNTS_BUFFER_SIZE: u64 = 16;
/// Half-open pixel rectangle: [min_x, min_y, max_x, max_y].
pub(super) const HUD_RECT: [u32; 4] = [2, 2, 10, 6];
pub(super) const HUD_COLOR: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
pub(super) const HISTOGRAM_COLOR: [f32; 4] = [1.0; 4];

#[derive(Clone)]
pub(super) struct Effects {
    image_layout: wgpu::BindGroupLayout,
    blur_horizontal: wgpu::ComputePipeline,
    blur_vertical: wgpu::ComputePipeline,
    complement: wgpu::ComputePipeline,
    hud: wgpu::ComputePipeline,
    accumulate_layout: wgpu::BindGroupLayout,
    accumulate: wgpu::ComputePipeline,
    luminance_layout: wgpu::BindGroupLayout,
    luminance: wgpu::ComputePipeline,
    normalize_layout: wgpu::BindGroupLayout,
    normalize: wgpu::ComputePipeline,
    histogram: wgpu::RenderPipeline,
}

impl Effects {
    pub fn new(device: &wgpu::Device) -> Self {
        let sampled_image = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("comparison-image-layout"),
            entries: &[
                sampled_image,
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
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("comparison-image-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/image.wgsl").into()),
        });
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("comparison-image-pipeline-layout"),
                bind_group_layouts: &[Some(&image_layout)],
                immediate_size: 0,
            });
        let image_pipeline = |entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&image_pipeline_layout),
                module: &image_shader,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let blur_horizontal = image_pipeline("blur_horizontal");
        let blur_vertical = image_pipeline("blur_vertical");
        let complement = image_pipeline("complement");
        let hud = image_pipeline("hud");

        let accumulate_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("comparison-accumulate-layout"),
            entries: &[
                sampled_image,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    ..sampled_image
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
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
        let accumulate_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("comparison-accumulate-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/accumulate.wgsl").into()),
        });
        let accumulate_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("comparison-accumulate-pipeline-layout"),
                bind_group_layouts: &[Some(&accumulate_layout)],
                immediate_size: 0,
            });
        let accumulate = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("comparison-accumulate"),
            layout: Some(&accumulate_pipeline_layout),
            module: &accumulate_shader,
            entry_point: Some("accumulate"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let storage_counts = wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(COUNTS_BUFFER_SIZE),
            },
            count: None,
        };
        let luminance_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("comparison-luminance-layout"),
            entries: &[sampled_image, storage_counts],
        });
        let luminance_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("comparison-luminance-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/luminance.wgsl").into()),
        });
        let luminance_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("comparison-luminance-pipeline-layout"),
                bind_group_layouts: &[Some(&luminance_layout)],
                immediate_size: 0,
            });
        let luminance = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("comparison-luminance"),
            layout: Some(&luminance_pipeline_layout),
            module: &luminance_shader,
            entry_point: Some("luminance"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let normalize_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("comparison-normalize-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                ..storage_counts
            }],
        });
        let normalize_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("comparison-normalize-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/normalize.wgsl").into()),
        });
        let normalize_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("comparison-normalize-pipeline-layout"),
                bind_group_layouts: &[Some(&normalize_layout)],
                immediate_size: 0,
            });
        let normalize = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("comparison-normalize"),
            layout: Some(&normalize_pipeline_layout),
            module: &normalize_shader,
            entry_point: Some("normalize_counts"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let histogram_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("comparison-histogram-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/histogram.wgsl").into()),
        });
        let histogram_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("comparison-histogram-pipeline-layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });
        let histogram = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("comparison-histogram"),
            layout: Some(&histogram_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &histogram_shader,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &histogram_shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            image_layout,
            blur_horizontal,
            blur_vertical,
            complement,
            hud,
            accumulate_layout,
            accumulate,
            luminance_layout,
            luminance,
            normalize_layout,
            normalize,
            histogram,
        }
    }

    /// Three equal taps at x - 1, x, x + 1, clamped to the source edges.
    pub fn encode_blur_horizontal(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        destination: &wgpu::TextureView,
    ) {
        self.encode_image(device, encoder, &self.blur_horizontal, source, destination);
    }

    /// Three equal taps at y - 1, y, y + 1, clamped to the source edges.
    pub fn encode_blur_vertical(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        destination: &wgpu::TextureView,
    ) {
        self.encode_image(device, encoder, &self.blur_vertical, source, destination);
    }

    /// Linear premultiplied complement: (alpha - rgb, alpha).
    pub fn encode_complement(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        destination: &wgpu::TextureView,
    ) {
        self.encode_image(device, encoder, &self.complement, source, destination);
    }

    /// Copy the image and replace HUD_RECT with opaque HUD_COLOR, clipped to its extent.
    pub fn encode_hud(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        destination: &wgpu::TextureView,
    ) {
        self.encode_image(device, encoder, &self.hud, source, destination);
    }

    /// Average current and exact retained history in linear premultiplied RGBA.
    /// The caller supplies initialized inputs and a distinct, equally sized output allocation.
    /// Inputs require TEXTURE_BINDING; the output requires STORAGE_BINDING.
    pub fn encode_accumulate(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        current: &wgpu::TextureView,
        history: &wgpu::TextureView,
        destination: &wgpu::TextureView,
    ) {
        let [width, height] = rgba32_extent(current);
        assert_eq!(rgba32_extent(history), [width, height]);
        assert_eq!(rgba32_extent(destination), [width, height]);
        assert_ne!(
            current.texture(),
            destination.texture(),
            "accumulation requires an output distinct from current"
        );
        assert_ne!(
            history.texture(),
            destination.texture(),
            "accumulation requires an output distinct from history"
        );
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("comparison-accumulate-bindings"),
            layout: &self.accumulate_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(current),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(history),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(destination),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("comparison-accumulate"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.accumulate);
        pass.set_bind_group(0, &bindings, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }

    /// Accumulate Rec.709 luminance counts into four u32s in a caller-zeroed 16-byte buffer.
    /// Luminance uses premultiplied rgb directly, clamps to [0, 1], then min(floor(4*y), 3).
    pub fn encode_luminance(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        counts: &wgpu::Buffer,
    ) {
        let [width, height] = rgba32_extent(source);
        assert_eq!(counts.size(), COUNTS_BUFFER_SIZE);
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("comparison-luminance-bindings"),
            layout: &self.luminance_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: counts.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("comparison-luminance"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.luminance);
        pass.set_bind_group(0, &bindings, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }

    /// Normalize a fresh 16-byte COPY-from-raw successor in place to four bitcast f32s.
    /// The caller must copy the retained raw counts before this call; raw is never bound here.
    /// A single invocation captures the total before any write. An empty total produces zeros.
    pub fn encode_normalize(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        normalized: &wgpu::Buffer,
    ) {
        assert_eq!(normalized.size(), COUNTS_BUFFER_SIZE);
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("comparison-normalize-bindings"),
            layout: &self.normalize_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: normalized.as_entire_binding(),
            }],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("comparison-normalize"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.normalize);
        pass.set_bind_group(0, &bindings, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }

    /// Draw four adjacent opaque white bars from the normalized buffer as actual vertex input.
    /// Bin i covers [i/4, (i+1)/4) of the width and rises from the bottom by its frequency.
    /// Pixel centers at or below the bar's top edge are covered (y + 0.5 >= H * (1 - frequency)).
    /// The caller-initialized background is loaded/preserved.
    /// The buffer needs VERTEX usage and the target needs RENDER_ATTACHMENT usage.
    pub fn encode_histogram(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        normalized: &wgpu::Buffer,
        destination: &wgpu::TextureView,
    ) {
        assert_eq!(normalized.size(), COUNTS_BUFFER_SIZE);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("comparison-histogram"),
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
        pass.set_pipeline(&self.histogram);
        pass.set_vertex_buffer(0, normalized.slice(0..COUNTS_BUFFER_SIZE));
        pass.draw(0..6, 0..HISTOGRAM_BINS);
    }

    fn encode_image(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        source: &wgpu::TextureView,
        destination: &wgpu::TextureView,
    ) {
        let [width, height] = rgba32_extent(source);
        assert_eq!(rgba32_extent(destination), [width, height]);
        assert_ne!(
            source.texture(),
            destination.texture(),
            "image effects require distinct resources"
        );
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("comparison-image-bindings"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(destination),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("comparison-image-effect"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bindings, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
}

fn rgba32_extent(view: &wgpu::TextureView) -> [u32; 2] {
    let texture = view.texture();
    assert_eq!(texture.format(), wgpu::TextureFormat::Rgba32Float);
    assert_eq!(texture.dimension(), wgpu::TextureDimension::D2);
    assert_eq!(texture.sample_count(), 1);
    assert_eq!(texture.mip_level_count(), 1);
    assert_eq!(texture.depth_or_array_layers(), 1);
    [texture.width(), texture.height()]
}
