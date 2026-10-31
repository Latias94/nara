use super::*;
use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureAllocation,
    WgpuFeatureDefinition, WgpuFeatureEncodeContext, WgpuFeaturePrepareContext, WgpuProgramTexture,
    WgpuProgramTextureDescriptor, WgpuRenderFeature, WgpuRenderHost, WgpuRenderProgramBuilder,
    WgpuRenderProgramError, WgpuSubmissionPrefix,
};
use nara_app::{App, RuntimeCandidateRetirementState, RuntimeInstance};
use nara_core::Color;
use nara_render::{
    ExtractedView, ExtractedViews, RenderTarget, ViewLoadOp, ViewMapping2d, ViewportRect,
    WorldCanvas, build_render_frame_packet_with_world_canvas,
};
use nara_window::{Window, WindowId, WindowResolution};

struct SceneColor;
struct CompositeColor;

fn canvas_frame(width: u32, height: u32, scale: f64) -> WorldCanvasFrame {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let generation = runtime.generation();
    let mut retirement = runtime.begin_retirement();
    while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
        retirement.drive_retirement();
    }
    let window = Window::new(
        "graph canvas",
        WindowResolution::new(width, height).with_scale_factor(scale),
    );
    let viewport = ViewportRect::new(0, 0, 8, 4).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: nara_render::RenderViewId::try_acquire().unwrap(),
        camera_entity: nara_ecs::Entity::PLACEHOLDER,
        target: RenderTarget::Window(WindowId::PRIMARY),
        viewport,
        mapping: ViewMapping2d::orthographic(nara_core::Mat3::IDENTITY, 4.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    build_render_frame_packet_with_world_canvas(
        generation,
        1,
        Some(WindowId::PRIMARY),
        [&window],
        &views,
        Some(
            WorldCanvas::new(8, 4)
                .unwrap()
                .with_bar_color(Color::rgba(0.25, 0.125, 0.5, 1.0))
                .unwrap(),
        ),
    )
    .unwrap()
    .unwrap()
    .world_canvas()
    .unwrap()
}

fn descriptor(
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("graph-canvas-color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        transfer: if format.is_srgb() {
            TextureTransfer::Srgb
        } else {
            TextureTransfer::Linear
        },
        alpha: TextureAlpha::Premultiplied,
    }
}

#[test]
fn canvas_capture_uses_distinct_semantics_without_send_bounds_or_native_resources() {
    type Source = std::rc::Rc<()>;
    type Dest = std::cell::Cell<()>;
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(CanvasCompositeFeature::<Source, Dest>::new()),
        )
        .unwrap();
    let frame = canvas_frame(63, 61, 1.5);
    let mut builder = WgpuRenderProgramBuilder::new(
        RenderFrameSourceGeneration::try_acquire().unwrap(),
        1,
        GraphLimits::default(),
    )
    .unwrap();
    let source = builder
        .texture::<Source>(descriptor(8, 4, WORLD_CANVAS_FORMAT))
        .unwrap();
    let target = builder
        .texture::<Dest>(descriptor(63, 61, WORLD_CANVAS_FORMAT))
        .unwrap();
    let capture = Arc::new(CanvasCompositeCapture::new(
        source.clone(),
        target.clone(),
        frame,
    ));
    assert_eq!(capture.frame, frame);
    builder
        .operation(
            1,
            &instance,
            Arc::clone(&capture),
            &[capture.source.read(AccessRole::Sampled)],
            &[capture.target.clear(
                crate::clear_color_to_wgpu(frame.bar_color()),
                AccessRole::Attachment,
            )],
        )
        .unwrap();
    builder
        .initialize(
            9,
            &[source.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
        )
        .unwrap();
    builder.finish(&[target.output()]).unwrap();
    assert_eq!(host.retained_resource_bytes(), 0);
}

// Exact input bytes exercise opaque controls, nontrivial sRGB, translucent premultiplied
// color and transparent holes. Upload and readback use ordinary Features, with no test renderer.
fn source_pixel(x: u32, y: u32) -> [u8; 4] {
    match (x + 3 * y) % 5 {
        0 => [255, 0, 0, 255],
        1 => [0, 255, 0, 255],
        2 => [128, 64, 32, 128],
        3 => [0, 0, 0, 0],
        _ => [64, 128, 188, 192],
    }
}

struct SourceUpload;
struct SourcePrepared {
    target: WgpuProgramTexture<SceneColor>,
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

impl WgpuRenderFeature for SourceUpload {
    type Capture = WgpuProgramTexture<SceneColor>;
    type State = ();
    type Prepared = SourcePrepared;
    type Report = ();

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn prepare(
        &self,
        _: &mut (),
        target: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let mut bytes = [0u8; 4 * 256];
        for y in 0..4 {
            for x in 0..8 {
                let offset = (y * 256 + x * 4) as usize;
                bytes[offset..offset + 4].copy_from_slice(&source_pixel(x, y));
            }
        }
        let allocation = context.reserve_bytes(bytes.len() as u64)?;
        let buffer = crate::allocation::create_checked_buffer_with_data(
            context.device(),
            "canvas-test-source-upload",
            &bytes,
            wgpu::BufferUsages::COPY_SRC,
        )?;
        context.retain((buffer.clone(), allocation.clone()))?;
        Ok(SourcePrepared {
            target: target.clone(),
            buffer,
            _allocation: allocation,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let target = context.resources().texture_native(&prepared.target)?;
        context.encoder().copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &prepared.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(4),
                },
            },
            target.as_image_copy(),
            target.size(),
        );
        Ok(())
    }
}

// Same public Feature readback pattern as the independent render-program consumer. The existing
// quad readback fixture owns a separate test device, so it cannot observe this real Host's output.
struct PixelReadback;
struct ReadbackPrepared {
    target: WgpuProgramTexture<CompositeColor>,
    report: PixelReport,
}

#[derive(Clone)]
struct PixelReport {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

impl WgpuRenderFeature for PixelReadback {
    type Capture = WgpuProgramTexture<CompositeColor>;
    type State = ();
    type Prepared = ReadbackPrepared;
    type Report = PixelReport;

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn prepare(
        &self,
        _: &mut (),
        target: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let size = target.descriptor().native.size;
        let stride = (size.width * 4).div_ceil(256) * 256;
        let bytes = u64::from(stride) * u64::from(size.height);
        let allocation = context.reserve_bytes(bytes)?;
        let device = context.device();
        let buffer = checked(device, "canvas-test-readback", || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("canvas-test-readback"),
                size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        })?;
        context.retain((buffer.clone(), allocation.clone()))?;
        Ok(ReadbackPrepared {
            target: target.clone(),
            report: PixelReport {
                device: device.clone(),
                buffer,
                _allocation: allocation,
            },
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        let target = context.resources().texture_native(&prepared.target)?;
        context.encoder().copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &prepared.report.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((target.width() * 4).div_ceil(256) * 256),
                    rows_per_image: Some(target.height()),
                },
            },
            target.size(),
        );
        Ok(prepared.report.clone())
    }
}

impl PixelReport {
    fn pixels(&self, width: u32, prefix: &WgpuSubmissionPrefix) -> Vec<[u8; 4]> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            self.device.poll(wgpu::PollType::Poll).unwrap();
            match receiver.try_recv() {
                Ok(result) => {
                    result.unwrap();
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "canvas readback timed out"
                    );
                }
                Err(error) => panic!("canvas mapping callback disappeared: {error}"),
            }
            std::thread::yield_now();
        }
        let pixels = {
            let mapped = self.buffer.get_mapped_range(..).unwrap();
            mapped
                .chunks_exact((width * 4).div_ceil(256) as usize * 256)
                .flat_map(|row| {
                    row[..width as usize * 4]
                        .chunks_exact(4)
                        .map(|pixel| pixel.try_into().unwrap())
                })
                .collect()
        };
        self.buffer.unmap();
        while !prefix.poll().unwrap() {
            assert!(
                std::time::Instant::now() < deadline,
                "canvas prefix did not settle"
            );
            std::thread::yield_now();
        }
        pixels
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_canvas_color_and_automatic_cost_refuse_before_native_allocation() {
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(0);
    let producer = host.create_producer().unwrap();
    let canvas = host.create_feature_instance(
        &producer,
        &WgpuFeatureDefinition::new(CanvasCompositeFeature::<SceneColor, CompositeColor>::new()),
    ).unwrap();
    let frame = canvas_frame(64, 64, 1.0);
    let source_generation = RenderFrameSourceGeneration::try_acquire().unwrap();
    let build = |metadata_bytes, format| {
        let mut builder = WgpuRenderProgramBuilder::new(
            source_generation,
            1,
            GraphLimits {
                max_modeled_metadata_bytes: metadata_bytes,
                ..Default::default()
            },
        )
        .unwrap();
        let source = builder
            .texture::<SceneColor>(descriptor(8, 4, WORLD_CANVAS_FORMAT))
            .unwrap();
        let target = builder
            .texture::<CompositeColor>(descriptor(64, 64, format))
            .unwrap();
        builder
            .initialize(
                1,
                &[source.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        builder
            .operation(
                2,
                &canvas,
                Arc::new(CanvasCompositeCapture::new(
                    source.clone(),
                    target.clone(),
                    frame,
                )),
                &[source.read(AccessRole::Sampled)],
                &[target.clear(
                    crate::clear_color_to_wgpu(frame.bar_color()),
                    AccessRole::Attachment,
                )],
            )
            .unwrap();
        builder.finish(&[target.output()]).unwrap()
    };
    let envelope = build(64 * 1024 * 1024, WORLD_CANVAS_FORMAT).modeled_metadata_bytes();
    let program = build(envelope, WORLD_CANVAS_FORMAT);
    host.begin_frame(1).unwrap();
    let allocation = crate::allocation::AllocationFailureGuard::at("nara_program_texture");
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(matches!(
        error,
        crate::WgpuRenderHostError::Program(WgpuRenderProgramError::Graph(
            crate::GraphError::LimitExceeded(crate::LimitKind::MetadataBytes)
        ))
    ));
    assert!(!allocation.was_consumed());
    assert_eq!(host.retained_resource_bytes(), 0);
    drop(allocation);
    let unsupported = build(64 * 1024 * 1024, wgpu::TextureFormat::R8Unorm);
    let allocation = crate::allocation::AllocationFailureGuard::at("nara_program_texture");
    assert!(matches!(
        host.submit_program(&producer, &unsupported),
        Err(crate::WgpuRenderHostError::Program(
            WgpuRenderProgramError::Feature { .. }
        ))
    ));
    assert!(!allocation.was_consumed());
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_canvas_graph_preserves_exact_bounds_alpha_color_and_mapping_custody() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let canvas = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(CanvasCompositeFeature::<SceneColor, CompositeColor>::new()),
        )
        .unwrap();
    let upload = host
        .create_feature_instance(&producer, &WgpuFeatureDefinition::new(SourceUpload))
        .unwrap();
    let readback = host
        .create_feature_instance(&producer, &WgpuFeatureDefinition::new(PixelReadback))
        .unwrap();
    let source_generation = RenderFrameSourceGeneration::try_acquire().unwrap();
    let mut sequence = 0;
    for format in [
        WORLD_CANVAS_FORMAT,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        for (width, height, scale) in [(64, 64, 1.0), (63, 61, 1.5), (5, 3, 1.25), (1, 8, 1.0)] {
            sequence += 1;
            let frame = canvas_frame(width, height, scale);
            let stride = (width * 4).div_ceil(256) * 256;
            let output_bytes = u64::from(width * height * 4);
            let readback_bytes = u64::from(stride * height);
            // Exactly the two graph textures, source staging, mapping and readback. Any hidden
            // raster allocation would exceed the real shared preparation budget.
            host = host.with_resource_byte_limit(
                8 * 4 * 4 + 4 * 256 + CANVAS_MAPPING_BYTES + output_bytes + readback_bytes,
            );
            host.begin_frame(sequence).unwrap();
            let mut builder =
                WgpuRenderProgramBuilder::new(source_generation, sequence, GraphLimits::default())
                    .unwrap();
            let source = builder
                .texture::<SceneColor>(descriptor(8, 4, WORLD_CANVAS_FORMAT))
                .unwrap();
            let target = builder
                .texture::<CompositeColor>(descriptor(width, height, format))
                .unwrap();
            // Deliberately declare consumers first. Only exact resource edges establish order.
            builder
                .operation(
                    1,
                    &readback,
                    Arc::new(target.clone()),
                    &[target.read(AccessRole::CopySource)],
                    &[],
                )
                .unwrap();
            builder
                .operation(
                    2,
                    &canvas,
                    Arc::new(CanvasCompositeCapture::new(
                        source.clone(),
                        target.clone(),
                        frame,
                    )),
                    &[source.read(AccessRole::Sampled)],
                    &[target.clear(
                        crate::clear_color_to_wgpu(frame.bar_color()),
                        AccessRole::Attachment,
                    )],
                )
                .unwrap();
            builder
                .operation(
                    9,
                    &upload,
                    Arc::new(source.clone()),
                    &[],
                    &[source.clear(wgpu::Color::TRANSPARENT, AccessRole::CopyDestination)],
                )
                .unwrap();
            let program = builder.finish(&[target.output()]).unwrap();
            let outputs = host.submit_program(&producer, &program).unwrap();
            let pixels = outputs
                .report::<PixelReadback>(1, &readback)
                .unwrap()
                .pixels(width, outputs.submitted_prefix());
            assert_pixels(&pixels, frame, width, height, format);
            assert_eq!(
                host.retained_resource_bytes(),
                output_bytes + readback_bytes,
                "scratch, source staging and the guarded mapping must retire with the original prefix"
            );
            drop(outputs);
            assert_eq!(host.retained_resource_bytes(), 0);
        }
    }
    while !host.poll_retirement().unwrap() {
        std::thread::yield_now();
    }
}

fn assert_pixels(
    pixels: &[[u8; 4]],
    frame: WorldCanvasFrame,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) {
    let mapping = frame.mapping();
    let ppp = mapping.pixels_per_point();
    let content = mapping.content_rect();
    let min = content.min.map(|edge| edge * ppp);
    let max = content.max.map(|edge| edge * ppp);
    let bar = frame.bar_color();
    for y in 0..height {
        for x in 0..width {
            let center = [x as f32 + 0.5, y as f32 + 0.5];
            let linear = if (0..2).all(|axis| center[axis] >= min[axis] && center[axis] < max[axis])
            {
                let sx = ((center[0] - min[0]) / (max[0] - min[0]) * 8.0).floor() as u32;
                let sy = ((center[1] - min[1]) / (max[1] - min[1]) * 4.0).floor() as u32;
                let [r, g, b, a] = source_pixel(sx.min(7), sy.min(3));
                [
                    decode_srgb(r),
                    decode_srgb(g),
                    decode_srgb(b),
                    f32::from(a) / 255.0,
                ]
            } else {
                [bar.r, bar.g, bar.b, bar.a]
            };
            let mut expected: [u8; 4] = std::array::from_fn(|channel| {
                let value = linear[channel];
                let encoded = if channel < 3 && format.is_srgb() {
                    if value <= 0.0031308 {
                        value * 12.92
                    } else {
                        1.055 * value.powf(1.0 / 2.4) - 0.055
                    }
                } else {
                    value
                };
                (encoded * 255.0).round() as u8
            });
            if format == wgpu::TextureFormat::Bgra8UnormSrgb {
                expected.swap(0, 2);
            }
            let actual = pixels[(y * width + x) as usize];
            assert!(
                actual
                    .into_iter()
                    .zip(expected)
                    .all(|(a, e)| a.abs_diff(e) <= 1),
                "{format:?} {width}x{height} pixel ({x}, {y}): {actual:?} != {expected:?}; content {min:?}..{max:?}"
            );
        }
    }
}

fn decode_srgb(value: u8) -> f32 {
    let value = f32::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
