//! Independent color grading through public production Feature APIs.
//!
//! Declare a sampled source read and an initialized destination attachment write. The ports
//! have equal actual extents and may independently use RGBA8/BGRA8 unorm or sRGB formats.
//! Both carry premultiplied alpha with transfer matching the native format. Native sRGB views
//! decode RGB into linear light; native sRGB attachments encode it on output. Alpha is linear.
//!
//! Grading unpremultiplies nonzero-alpha pixels, applies exposure, Rec. 709 luminance-based
//! saturation, then contrast about linear 0.5, clamps straight RGB to [0, 1], and premultiplies
//! again. Alpha is preserved, and alpha-zero pixels produce transparent black. All parameters
//! and exact ports belong to the capture; this package has no dependency on the blur package.

use std::marker::PhantomData;

use nara_render_wgpu::{
    TextureAlpha, TextureTransfer, WgpuDeviceRequirements, WgpuFeatureAllocation,
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuProgramTexture, WgpuProgramTextureDescriptor, WgpuRenderFeature, WgpuRenderProgramError,
};

const UNIFORM_BYTES: u64 = 16;

/// Captured linear-light grading controls, applied in exposure/saturation/contrast order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradeParameters {
    /// Exposure multiplier is `2^exposure_stops`; the admitted range is [-16, 16].
    pub exposure_stops: f32,
    /// Zero produces grayscale, one preserves saturation; the admitted range is [0, 4].
    pub saturation: f32,
    /// Slope about linear 0.5; one is unchanged. The admitted range is [0, 4].
    pub contrast: f32,
}

impl Default for GradeParameters {
    fn default() -> Self {
        Self {
            exposure_stops: 0.0,
            saturation: 1.0,
            contrast: 1.0,
        }
    }
}

impl GradeParameters {
    fn validate(self) -> Result<(), WgpuRenderProgramError> {
        if !self.exposure_stops.is_finite()
            || !(-16.0..=16.0).contains(&self.exposure_stops)
            || !self.saturation.is_finite()
            || !(0.0..=4.0).contains(&self.saturation)
            || !self.contrast.is_finite()
            || !(0.0..=4.0).contains(&self.contrast)
        {
            return Err(WgpuRenderProgramError::feature(
                "grade requires finite exposure in [-16, 16] and saturation/contrast in [0, 4]",
            ));
        }
        Ok(())
    }
}

/// An immutable adapter generic over independent or identical semantic port marker types.
///
/// Even `Grade<S, S>` writes a distinct graph resource version from its sampled source.
pub struct Grade<Source: 'static, Destination: 'static>(PhantomData<fn() -> (Source, Destination)>);

impl<Source: 'static, Destination: 'static> Grade<Source, Destination> {
    #[must_use]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<Source: 'static, Destination: 'static> Default for Grade<Source, Destination> {
    fn default() -> Self {
        Self::new()
    }
}

/// Replayable CPU parameters and exact typed ports for one grading operation.
pub struct GradeCapture<Source: 'static, Destination: 'static> {
    pub source: WgpuProgramTexture<Source>,
    pub destination: WgpuProgramTexture<Destination>,
    pub parameters: GradeParameters,
}

impl<Source: 'static, Destination: 'static> GradeCapture<Source, Destination> {
    fn validate(&self) -> Result<(), WgpuRenderProgramError> {
        self.parameters.validate()?;
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
                "grade source and destination must have the same actual extent",
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
            "grade requires an RGBA8/BGRA8 unorm or sRGB color port",
        ));
    }
    let transfer = if native.format.has_srgb_suffix() {
        TextureTransfer::Srgb
    } else {
        TextureTransfer::Linear
    };
    if descriptor.transfer != transfer || descriptor.alpha != TextureAlpha::Premultiplied {
        return Err(WgpuRenderProgramError::feature(
            "grade requires native color transfer and linear-premultiplied alpha semantics",
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
            "grade requires whole single-sample, single-mip 2D ports with their declared usages",
        ));
    }
    Ok(())
}

/// Host-owned native cache for this exact instance and device epoch.
#[derive(Default)]
pub struct GradeState {
    pipeline: Option<GradePipeline>,
}

#[derive(Clone)]
struct GradePipeline {
    destination_format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl GradePipeline {
    fn new(device: &wgpu::Device, destination_format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("external-grade-layout"),
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
            label: Some("external-grade-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("external-grade-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("grade.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("external-grade-pipeline"),
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
                    format: destination_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            destination_format,
            layout,
            pipeline,
        }
    }
}

/// Submission-local bindings, payload and allocation guard retained by the original Host.
pub struct GradePrepared<Destination: 'static> {
    destination: WgpuProgramTexture<Destination>,
    parameters: GradeParameters,
    pipeline: GradePipeline,
    bindings: wgpu::BindGroup,
    _uniform: wgpu::Buffer,
    _uniform_allocation: WgpuFeatureAllocation,
}

/// Typed evidence from successful complete-group publication, with the original parameters.
pub struct GradeReport<Destination: 'static> {
    pub destination: WgpuProgramTexture<Destination>,
    pub parameters: GradeParameters,
}

impl<Source: 'static, Destination: 'static> WgpuRenderFeature for Grade<Source, Destination> {
    type Capture = GradeCapture<Source, Destination>;
    type State = GradeState;
    type Prepared = GradePrepared<Destination>;
    type Report = GradeReport<Destination>;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        // One sampled texture, one 16-byte uniform and one color attachment use baseline limits.
        // The admitted unorm/sRGB formats require no optional feature or float filtering.
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
        Ok(GradeState::default())
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
        let allocation = context.reserve_bytes(UNIFORM_BYTES)?;
        let format = capture.destination.descriptor().native.format;
        let pipeline = match &state.pipeline {
            Some(pipeline) if pipeline.destination_format == format => pipeline.clone(),
            _ => GradePipeline::new(context.device(), format),
        };
        let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("external-grade-captured-parameters"),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        let bytes = [
            capture.parameters.exposure_stops.exp2(),
            capture.parameters.saturation,
            capture.parameters.contrast,
            0.0,
        ]
        .map(f32::to_le_bytes);
        uniform
            .get_mapped_range_mut(..)
            .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?
            .copy_from_slice(bytes.as_flattened());
        uniform.unmap();
        let bindings = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("external-grade-bindings"),
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
        // The mapped input needs no queue write; Prepared owns its exact allocation through use.
        Ok(GradePrepared {
            destination: capture.destination.clone(),
            parameters: capture.parameters,
            pipeline,
            bindings,
            _uniform: uniform,
            _uniform_allocation: allocation,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let destination = resources.texture(&prepared.destination)?;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("external-grade-pass"),
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
        }
        Ok(GradeReport {
            destination: prepared.destination.clone(),
            parameters: prepared.parameters,
        })
    }
}
