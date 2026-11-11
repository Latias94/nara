//! An independently authored, two-pass separable box blur using public production APIs.
//!
//! Declare a sampled read of the captured source and an initialized attachment write of the
//! destination. Both ports must have the same actual extent. The source may be world-raster
//! color or Canvas-resolved color: wiring selects the pixel space, and `radius` counts texels
//! in that exact input. The shader obtains its dimensions from the bound texture.
//!
//! RGBA8/BGRA8 unorm and sRGB ports carry premultiplied alpha. Native sRGB sampling decodes RGB
//! into linear light before either pass; native sRGB attachment writes encode RGB afterward.
//! Alpha stays linear. Both passes average linear premultiplied RGBA with clamped edge taps.
//! The private, same-sized `Rgba16Float` intermediate introduces half-float rounding without
//! an extra sRGB/unorm quantization between passes. No optional device features are needed.

use std::marker::PhantomData;

use nara_render_wgpu::{
    TextureAlpha, TextureTransfer, WgpuDeviceRequirements, WgpuFeatureAllocation,
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuProgramTexture, WgpuProgramTextureDescriptor, WgpuRenderFeature, WgpuRenderProgramError,
};

/// Bounds the per-pass work to at most 65 equally weighted, edge-clamped taps.
pub const MAX_RADIUS: u32 = 32;

const UNIFORM_BYTES: u64 = 16;
const SCRATCH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SCRATCH_TEXEL_BYTES: u64 = 8;

/// One immutable adapter definition, generic over the caller's semantic input/output markers.
///
/// `Blur<S, S>` is valid: the graph still requires distinct input and output resource versions.
/// One operation encodes a horizontal pass followed by a vertical pass.
pub struct Blur<Source: 'static, Destination: 'static>(PhantomData<fn() -> (Source, Destination)>);

impl<Source: 'static, Destination: 'static> Blur<Source, Destination> {
    #[must_use]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<Source: 'static, Destination: 'static> Default for Blur<Source, Destination> {
    fn default() -> Self {
        Self::new()
    }
}

/// Immutable ports and parameters for one blur operation.
pub struct BlurCapture<Source: 'static, Destination: 'static> {
    pub source: WgpuProgramTexture<Source>,
    pub destination: WgpuProgramTexture<Destination>,
    /// Radius in actual source texels, in `0..=MAX_RADIUS`; zero performs an identity blur.
    pub radius: u32,
}

impl<Source: 'static, Destination: 'static> BlurCapture<Source, Destination> {
    fn validate(&self) -> Result<(), WgpuRenderProgramError> {
        validate_color(
            self.source.descriptor(),
            wgpu::TextureUsages::TEXTURE_BINDING,
        )?;
        validate_color(
            self.destination.descriptor(),
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )?;
        if self.source.descriptor().native.size != self.destination.descriptor().native.size {
            return Err(WgpuRenderProgramError::feature(
                "blur source and destination must have the same actual extent",
            ));
        }
        if self.radius > MAX_RADIUS {
            return Err(WgpuRenderProgramError::feature(
                "blur radius exceeds MAX_RADIUS source texels",
            ));
        }
        Ok(())
    }
}

fn validate_color(
    descriptor: &WgpuProgramTextureDescriptor,
    usage: wgpu::TextureUsages,
) -> Result<(), WgpuRenderProgramError> {
    let native = &descriptor.native;
    if !matches!(
        native.format,
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        return Err(WgpuRenderProgramError::feature(
            "blur requires an RGBA8/BGRA8 unorm or sRGB color port",
        ));
    }
    let transfer = if native.format.has_srgb_suffix() {
        TextureTransfer::Srgb
    } else {
        TextureTransfer::Linear
    };
    if descriptor.transfer != transfer || descriptor.alpha != TextureAlpha::Premultiplied {
        return Err(WgpuRenderProgramError::feature(
            "blur requires native color transfer and linear-premultiplied alpha semantics",
        ));
    }
    if native.dimension != wgpu::TextureDimension::D2
        || native.size.width == 0
        || native.size.height == 0
        || native.size.depth_or_array_layers != 1
        || native.sample_count != 1
        || native.mip_level_count != 1
        || !native.view_formats.is_empty()
        || !native.usage.contains(usage)
    {
        return Err(WgpuRenderProgramError::feature(
            "blur requires whole single-sample, single-mip 2D ports with their declared usages",
        ));
    }
    Ok(())
}

/// Host-owned, instance-local and device-epoch-local native pipeline cache.
#[derive(Default)]
pub struct BlurState {
    pipelines: Option<BlurPipelines>,
}

#[derive(Clone)]
struct BlurPipelines {
    destination_format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
}

impl BlurPipelines {
    fn new(device: &wgpu::Device, destination_format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("external-blur-layout"),
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
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTES),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("external-blur-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("external-blur-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });
        Self {
            destination_format,
            horizontal: render_pipeline(
                device,
                &pipeline_layout,
                &module,
                "horizontal",
                SCRATCH_FORMAT,
            ),
            vertical: render_pipeline(
                device,
                &pipeline_layout,
                &module,
                "vertical",
                destination_format,
            ),
            layout,
        }
    }

    fn bindings(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        uniform: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("external-blur-bindings"),
            layout: &self.layout,
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
        })
    }
}

fn render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    fragment: &'static str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("external-blur-pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Owns every private payload and binding needed by this original operation's submission.
pub struct BlurPrepared<Destination: 'static> {
    destination: WgpuProgramTexture<Destination>,
    source_extent: wgpu::Extent3d,
    radius: u32,
    pipelines: BlurPipelines,
    horizontal_bindings: wgpu::BindGroup,
    vertical_bindings: wgpu::BindGroup,
    scratch_view: wgpu::TextureView,
    _scratch: wgpu::Texture,
    _scratch_allocation: WgpuFeatureAllocation,
    _uniform: wgpu::Buffer,
    _uniform_allocation: WgpuFeatureAllocation,
}

/// Typed evidence returned only when the Host publishes the complete output group.
pub struct BlurReport<Destination: 'static> {
    pub destination: WgpuProgramTexture<Destination>,
    pub source_extent: wgpu::Extent3d,
    pub radius: u32,
}

impl<Source: 'static, Destination: 'static> WgpuRenderFeature for Blur<Source, Destination> {
    type Capture = BlurCapture<Source, Destination>;
    type State = BlurState;
    type Prepared = BlurPrepared<Destination>;
    type Report = BlurReport<Destination>;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        // Each pass uses one sampled texture, one 16-byte uniform and one color attachment.
        // All four public formats and the Rgba16Float scratch fit the stock device baseline.
        WgpuDeviceRequirements::new(wgpu::Features::empty(), wgpu::Limits::default())
    }

    fn preflight(
        &self,
        capture: &Self::Capture,
        _context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        capture.validate()
    }

    fn create_state(
        &self,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::State, WgpuRenderProgramError> {
        Ok(BlurState::default())
    }

    fn prepare(
        &self,
        state: &mut Self::State,
        capture: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let resources = context.resources();
        let source = resources.texture(&capture.source)?;
        resources.texture(&capture.destination)?;
        let source_extent = capture.source.descriptor().native.size;
        let scratch_bytes = u64::from(source_extent.width)
            .checked_mul(u64::from(source_extent.height))
            .and_then(|pixels| pixels.checked_mul(SCRATCH_TEXEL_BYTES))
            .ok_or(WgpuRenderProgramError::ResourceCapacity)?;
        let scratch_allocation = context.reserve_bytes(scratch_bytes)?;
        let uniform_allocation = context.reserve_bytes(UNIFORM_BYTES)?;

        let format = capture.destination.descriptor().native.format;
        let pipelines = match &state.pipelines {
            Some(pipelines) if pipelines.destination_format == format => pipelines.clone(),
            _ => BlurPipelines::new(context.device(), format),
        };
        let scratch = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("external-blur-linear-intermediate"),
            size: source_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCRATCH_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let scratch_view = scratch.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("external-blur-captured-radius"),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        let bytes = [capture.radius, 0, 0, 0].map(u32::to_le_bytes);
        uniform
            .get_mapped_range_mut(..)
            .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?
            .copy_from_slice(bytes.as_flattened());
        uniform.unmap();

        let horizontal_bindings = pipelines.bindings(context.device(), source, &uniform);
        let vertical_bindings = pipelines.bindings(context.device(), &scratch_view, &uniform);
        state.pipelines = Some(pipelines.clone());
        // Mapped initialization performs no queue upload. Host retains Prepared before encoding,
        // keeping both payload guards and the exact bindings alive through the original use.
        Ok(BlurPrepared {
            destination: capture.destination.clone(),
            source_extent,
            radius: capture.radius,
            pipelines,
            horizontal_bindings,
            vertical_bindings,
            scratch_view,
            _scratch: scratch,
            _scratch_allocation: scratch_allocation,
            _uniform: uniform,
            _uniform_allocation: uniform_allocation,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let destination = resources.texture(&prepared.destination)?;
        draw(
            encoder,
            &prepared.scratch_view,
            &prepared.pipelines.horizontal,
            &prepared.horizontal_bindings,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        // Shared graph initialization already cleared or preserved this distinct destination.
        draw(
            encoder,
            destination,
            &prepared.pipelines.vertical,
            &prepared.vertical_bindings,
            wgpu::LoadOp::Load,
        );
        Ok(BlurReport {
            destination: prepared.destination.clone(),
            source_extent: prepared.source_extent,
            radius: prepared.radius,
        })
    }
}

fn draw(
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bindings: &wgpu::BindGroup,
    load: wgpu::LoadOp<wgpu::Color>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("external-blur-pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: destination,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bindings, &[]);
    pass.draw(0..3, 0..1);
}
