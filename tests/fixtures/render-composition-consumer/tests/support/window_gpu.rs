//! Small native sampling adapter for the two-window consumer. The Host submits every command.

use std::{
    error::Error,
    io,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use nara_core::Mat3;
use nara_ecs::Entity;
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFramePacket, RenderFrameSourceGeneration,
    RenderImagePacket, RenderTarget, RenderViewId, ViewLoadOp, ViewMapping2d, ViewportRect,
    build_image_render_frame_packet,
};
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDeclaration,
    WgpuFeatureDefinition, WgpuFeatureEncodeContext, WgpuFeatureInstance,
    WgpuFeaturePrepareContext, WgpuImageOutput, WgpuPolicyBindingContext, WgpuPolicyBuildContext,
    WgpuPolicyFrameFacts, WgpuPolicyProgram, WgpuProgramTexture, WgpuProgramTextureDescriptor,
    WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderHost, WgpuRenderPolicy,
    WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub const WIDTH: u32 = 32;
pub const HEIGHT: u32 = 16;
pub const RED: [u8; 4] = [255, 0, 0, 255];
pub const YELLOW: [u8; 4] = [255, 255, 0, 255];
pub const GREEN: [u8; 4] = [0, 255, 0, 255];
pub const BLUE: [u8; 4] = [0, 0, 255, 255];

/// A and B publish this same semantic port; the image consumer imports their exact versions.
pub struct StreamColor;
pub struct CompositeColor;

pub struct ClearPolicy;

impl WgpuRenderPolicy for ClearPolicy {
    type Capture = [u8; 4];
    type Bindings = ();

    fn bind(
        &self,
        _: &Self::Capture,
        _: &mut WgpuPolicyBindingContext<'_>,
    ) -> std::result::Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn build(
        &self,
        _: &(),
        color: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> std::result::Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut builder = builder(context.packet())?;
        let target = builder.texture::<StreamColor>(descriptor(context.format()))?;
        builder.initialize(
            1,
            &[target.clear(
                wgpu::Color {
                    r: f64::from(color[0]) / 255.0,
                    g: f64::from(color[1]) / 255.0,
                    b: f64::from(color[2]) / 255.0,
                    a: f64::from(color[3]) / 255.0,
                },
                AccessRole::Attachment,
            )],
        )?;
        WgpuPolicyProgram::new(
            builder.finish(&[target.output()])?,
            &target,
            WgpuPolicyFrameFacts::default(),
        )
    }
}

pub struct CompositeCapture {
    pub old_a: WgpuProgramTextureOutput<StreamColor>,
    pub current_b: WgpuProgramTextureOutput<StreamColor>,
}

pub struct CompositePolicy {
    feature: WgpuFeatureDeclaration<SplitImage>,
}

impl CompositePolicy {
    pub fn new() -> Self {
        Self {
            feature: WgpuFeatureDefinition::new(SplitImage).declare(),
        }
    }
}

impl WgpuRenderPolicy for CompositePolicy {
    type Capture = CompositeCapture;
    type Bindings = Arc<WgpuFeatureInstance<SplitImage>>;

    fn bind(
        &self,
        _: &CompositeCapture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> std::result::Result<Self::Bindings, WgpuRenderProgramError> {
        context.feature(&self.feature)
    }

    fn build(
        &self,
        feature: &Self::Bindings,
        capture: &CompositeCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> std::result::Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut builder = builder(context.packet())?;
        let a = builder.import_texture(&capture.old_a)?;
        let b = builder.import_texture(&capture.current_b)?;
        let target = builder.texture::<CompositeColor>(descriptor(context.format()))?;
        builder.operation(
            1,
            feature,
            Arc::new(SplitCapture {
                a: a.clone(),
                b: b.clone(),
                target: target.clone(),
            }),
            &[a.read(AccessRole::Sampled), b.read(AccessRole::Sampled)],
            &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )?;
        WgpuPolicyProgram::new(
            builder.finish(&[target.output()])?,
            &target,
            WgpuPolicyFrameFacts::default(),
        )
    }
}

fn builder(
    packet: &RenderFramePacket,
) -> std::result::Result<WgpuRenderProgramBuilder, WgpuRenderProgramError> {
    if packet.target().extent() != Extent2d::new(WIDTH, HEIGHT).unwrap()
        || packet.views().len() != 1
        || packet.world_canvas().is_some()
    {
        return Err(WgpuRenderProgramError::feature(
            "the window consumer accepts one 32x16 image View without a fixed canvas",
        ));
    }
    WgpuRenderProgramBuilder::new(
        packet.source_generation(),
        packet.frame_index(),
        GraphLimits {
            max_resources: 3,
            max_operations: 1,
            max_accesses: 3,
            max_outputs: 1,
            max_texture_dimension_2d: WIDTH,
            max_modeled_resource_bytes: u64::from(WIDTH * HEIGHT * 4 * 3),
            max_modeled_metadata_bytes: 1024 * 1024,
            ..GraphLimits::default()
        },
    )
}

fn descriptor(format: wgpu::TextureFormat) -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("streams-window-image"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
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
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    }
}

pub fn packet(
    source: RenderFrameSourceGeneration,
    view: RenderViewId,
    frame: u64,
    target: RenderImagePacket,
) -> Result<RenderFramePacket> {
    let viewport = ViewportRect::from_extent(target.extent()).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: view,
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Image(target.image()),
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, HEIGHT as f32, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(nara_core::Color::BLACK),
    });
    build_image_render_frame_packet(source, frame, &views, target)?
        .ok_or_else(|| io::Error::other("the image View did not produce a packet").into())
}

pub struct SplitImage;

pub struct SplitCapture {
    a: WgpuProgramTexture<StreamColor>,
    b: WgpuProgramTexture<StreamColor>,
    target: WgpuProgramTexture<CompositeColor>,
}

pub struct PreparedSplit {
    target: WgpuProgramTexture<CompositeColor>,
    painter: ImagePainter,
    group: wgpu::BindGroup,
}

impl WgpuRenderFeature for SplitImage {
    type Capture = SplitCapture;
    type State = Option<ImagePainter>;
    type Prepared = PreparedSplit;
    type Report = ();

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> std::result::Result<Self::State, WgpuRenderProgramError> {
        Ok(None)
    }

    fn prepare(
        &self,
        state: &mut Self::State,
        capture: &SplitCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> std::result::Result<PreparedSplit, WgpuRenderProgramError> {
        let format = capture.target.descriptor().native.format;
        if state
            .as_ref()
            .is_none_or(|painter| painter.format != format)
        {
            *state = Some(ImagePainter::new(context.device(), format, true));
        }
        let painter = state.as_ref().unwrap();
        let resources = context.resources();
        let group = painter.bind(
            context.device(),
            resources.texture(&capture.a)?,
            resources.texture(&capture.b)?,
        );
        Ok(PreparedSplit {
            target: capture.target.clone(),
            painter: painter.clone(),
            group,
        })
    }

    fn encode(
        &self,
        prepared: &PreparedSplit,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> std::result::Result<(), WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        prepared.painter.draw(
            encoder,
            resources.texture(&prepared.target)?,
            &prepared.group,
        );
        Ok(())
    }
}

/// Frontend-only pipeline state. It never acquires a surface or submits a queue.
#[derive(Clone)]
pub struct ImagePainter {
    pub format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl ImagePainter {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, split: bool) -> Self {
        let entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("streams-window-sampling-layout"),
            entries: &[entry(0), entry(1)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("streams-window-sampling-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("streams-window-sampling-shader"),
            source: wgpu::ShaderSource::Wgsl(SAMPLE_SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("streams-window-sampling-pipeline"),
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
                entry_point: Some(if split { "split_image" } else { "image" }),
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

    pub fn bind(
        &self,
        device: &wgpu::Device,
        first: &wgpu::TextureView,
        second: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("streams-window-exact-image-bindings"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(first),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(second),
                },
            ],
        })
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        group: &wgpu::BindGroup,
    ) {
        self.draw_native(encoder, target, group, None);
    }

    pub fn draw_placed(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        group: &wgpu::BindGroup,
        destination: [u32; 4],
        clip: [u32; 4],
    ) {
        self.draw_native(encoder, target, group, Some((destination, clip)));
    }

    fn draw_native(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        group: &wgpu::BindGroup,
        placement: Option<([u32; 4], [u32; 4])>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("streams-window-exact-image-blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        if let Some(([x, y, width, height], [clip_x, clip_y, clip_width, clip_height])) = placement
        {
            pass.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
            pass.set_scissor_rect(clip_x, clip_y, clip_width, clip_height);
        }
        pass.draw(0..3, 0..1);
    }
}

const SAMPLE_SHADER: &str = r#"
@group(0) @binding(0) var first_image: texture_2d<f32>;
@group(0) @binding(1) var second_image: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position = positions[index];
    var output: VertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return output;
}

fn coordinates(uv: vec2<f32>, extent: vec2<u32>) -> vec2<i32> {
    return vec2<i32>(clamp(uv * vec2<f32>(extent), vec2<f32>(0.0), vec2<f32>(extent - vec2<u32>(1u))));
}

@fragment
fn image(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureLoad(first_image, coordinates(input.uv, textureDimensions(first_image)), 0);
}

@fragment
fn split_image(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.uv.x < 0.5 {
        let uv = vec2<f32>(input.uv.x * 2.0, input.uv.y);
        return textureLoad(first_image, coordinates(uv, textureDimensions(first_image)), 0);
    }
    let uv = vec2<f32>((input.uv.x - 0.5) * 2.0, input.uv.y);
    return textureLoad(second_image, coordinates(uv, textureDimensions(second_image)), 0);
}
"#;

/// Checks every RGBA8 pixel, separately waiting for the map callback and the original prefix.
pub fn assert_pixels(
    host: &mut WgpuRenderHost,
    output: &WgpuImageOutput,
    left: [u8; 4],
    right: [u8; 4],
) -> Result<usize> {
    assert_eq!(output.format(), wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(output.extent(), Extent2d::new(WIDTH, HEIGHT).unwrap());
    let row_bytes = WIDTH * 4;
    let padded_row =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let bytes = u64::from(padded_row) * u64::from(HEIGHT);
    let (readback, prefix) = host.submit_image_use(
        std::slice::from_ref(output),
        |device, _queue, encoder, views| {
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("streams-window-numeric-readback"),
                size: bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                views[0].texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
            );
            Ok(readback)
        },
    )?;
    let (sender, receiver) = mpsc::sync_channel(1);
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut mapped = false;
    loop {
        let complete = prefix.poll()?;
        // Prefix settlement can precede the independently requested mapping callback.
        host.with_image(output, |device, _, _| device.poll(wgpu::PollType::Poll))??;
        if !mapped {
            match receiver.try_recv() {
                Ok(result) => {
                    result?;
                    mapped = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(io::Error::other("the native map callback disappeared").into());
                }
            }
        }
        if mapped && complete {
            break;
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other("the native readback exceeded ten seconds").into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    {
        let pixels = readback.get_mapped_range(..)?;
        for row in pixels.chunks_exact(padded_row as usize) {
            for (x, pixel) in row[..row_bytes as usize].chunks_exact(4).enumerate() {
                let expected = if x < WIDTH as usize / 2 { left } else { right };
                assert_eq!(
                    pixel,
                    expected.as_slice(),
                    "the exact retained image pixel at x={x}"
                );
            }
        }
    }
    readback.unmap();
    Ok((WIDTH * HEIGHT) as usize)
}
