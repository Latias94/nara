use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use nara_app::{App, RuntimeInstance};
use nara_core::Mat3;
use nara_ecs::Entity;
use nara_window::{Window, WindowResolution};

use crate::{
    ExtractedView, ExtractedViews, RenderImage2d, RenderImagePacket, RenderTarget, RenderViewId,
    ViewLoadOp, ViewMapping2d, ViewportRect, WorldCanvas,
    build_render_frame_packet_with_world_canvas,
};

use super::*;

fn gpu() -> Renderer {
    Renderer::new(
        GpuContext::request(
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env()),
            None,
        )
        .unwrap(),
    )
}

fn frame(canvas: bool) -> (RenderInput, RenderInput, Extent2d) {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let extent = Extent2d::new(8, 8).unwrap();
    let window = Window::new(
        "test target",
        WindowResolution::new(extent.width, extent.height),
    );
    let canvas = canvas.then(|| {
        WorldCanvas::new(4, 2)
            .unwrap()
            .with_bar_color(Color::rgb(0.0, 0.0, 1.0))
            .unwrap()
    });
    let scene_extent = canvas.map_or(extent, WorldCanvas::extent);
    let viewport = ViewportRect::from_extent(scene_extent).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Window(window.id),
        order: 0,
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        load: ViewLoadOp::Clear(Color::rgba(1.0, 0.0, 0.0, 0.25)),
    });
    let window_frame = build_render_frame_packet_with_world_canvas(
        runtime.generation(),
        1,
        Some(window.id),
        [&window],
        &views,
        canvas,
    )
    .unwrap()
    .unwrap();
    let image = RenderImagePacket::new(RenderImage2d::allocate_handle().unwrap(), extent).unwrap();
    let image_frame = window_frame.with_image_target(image).unwrap();
    let inputs = (
        RenderInput::capture(runtime.world(), window_frame),
        RenderInput::capture(runtime.world(), image_frame),
        extent,
    );
    runtime.finish().unwrap();
    inputs
}

struct EditorOverlay(wgpu::RenderPipeline);

impl RenderPass for EditorOverlay {
    fn attachments(&self) -> PassAttachments {
        PassAttachments::OVERLAY
    }

    fn execute(&mut self, context: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError> {
        let mut pass = context.begin_color_pass("test editor overlay", wgpu::LoadOp::Load);
        pass.set_pipeline(&self.0);
        pass.set_scissor_rect(4, 4, 1, 1);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}

fn editor_overlay(device: &wgpu::Device, format: wgpu::TextureFormat) -> EditorOverlay {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            r"
            @vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
                let vertices = array(vec2f(-1., -1.), vec2f(3., -1.), vec2f(-1., 3.));
                return vec4f(vertices[i], 0., 1.);
            }
            @fragment fn fragment() -> @location(0) vec4f { return vec4f(0., 1., 0., 1.); }
        "
            .into(),
        ),
    });
    EditorOverlay(
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
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
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        }),
    )
}

struct RejectedOverlay;

impl RenderPass for RejectedOverlay {
    fn attachments(&self) -> PassAttachments {
        PassAttachments::OVERLAY
    }

    fn execute(&mut self, _: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError> {
        Err(WgpuRenderError::Validation(
            "test overlay rejected the frame".into(),
        ))
    }
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn direct_and_leased_targets_share_canvas_alpha_and_editor_overlay() {
    let mut renderer = gpu();
    let context = renderer.context().clone();
    let device = context.device();
    let queue = context.queue();
    let (direct, offscreen, extent) = frame(true);
    #[cfg(feature = "hud-runtime")]
    let (direct, offscreen) = {
        let hud = overlay_hud();
        (direct.with_hud(hud.clone()), offscreen.with_hud(hud))
    };
    let format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: size(extent),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let target = RenderTargetView {
        view: &view,
        format,
        extent,
    };
    renderer
        .set_editor_overlay(Some(Box::new(RejectedOverlay)))
        .unwrap();
    assert!(matches!(
        renderer.render_to_view(&direct, target),
        Err(WgpuRenderError::Validation(_))
    ));
    wait(device, None);
    renderer
        .set_editor_overlay(Some(Box::new(editor_overlay(device, format))))
        .unwrap();
    let report = renderer.render_to_view(&direct, target).unwrap();
    assert_eq!(report.work.created_buffer_calls, 0);
    renderer
        .set_editor_overlay(Some(Box::new(editor_overlay(
            device,
            WgpuImageOutput::FORMAT,
        ))))
        .unwrap();
    let (output, _) = renderer.render_to_image(&offscreen).unwrap();
    let mut pixels = read(device, queue, &texture, extent);
    for pixel in &mut pixels {
        pixel.swap(0, 2);
    }
    assert_eq!(
        pixels,
        read(device, queue, output.native_texture().unwrap(), extent)
    );
    assert_eq!(pixels[0], [0, 0, 255, 255]);
    assert_eq!(pixels[4 * 8 + 4], [0, 255, 0, 255]);
    #[cfg(feature = "hud-runtime")]
    assert_eq!(pixels[4 * 8 + 5], [255, 255, 0, 255]);
    let premultiplied = pixels[4 * 8 + 3];
    assert!(premultiplied[0].abs_diff(137) <= 1);
    assert_eq!(premultiplied[1..], [0, 0, 64]);
}

#[cfg(feature = "hud-runtime")]
fn overlay_hud() -> crate::hud::HudFrame {
    use nara_ui::kernel as ui;

    let snapshot = ui::UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            ui::UiRoot::new(
                ui::UiRootKey::runtime("overlay", "hud").unwrap(),
                ui::UiLayer::Base,
                ui::UiStyle::fill(),
            )
            .with_child(
                ui::UiNode::new(ui::UiStyle::absolute(4.0, 4.0, 2.0, 1.0))
                    .with_key(ui::UiLocalKey::runtime("overlay", "label").unwrap())
                    .with_visual(ui::UiVisual::solid(ui::UiColor::rgba(1.0, 1.0, 0.0, 1.0))),
            ),
        )
        .build()
        .unwrap();
    let transform =
        ui::UiSurfaceContentTransform::direct(ui::UiSize::new(8.0, 8.0).unwrap(), 1.0).unwrap();
    let environment = ui::UiEnvironment::for_surface(
        ui::UiSurfaceId::new(1).unwrap(),
        transform,
        ui::UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();
    let frame = ui::UiCompiler::new(
        nara_ui::text::bundled_font_face().unwrap(),
        Default::default(),
    )
    .unwrap()
    .compile(&snapshot, environment, ui::UiTheme::default())
    .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    crate::hud::HudFrame::new(frame, [])
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn output_waits_for_both_queue_and_import_owner_then_reuses_texture() {
    let mut renderer = gpu();
    let context = renderer.context().clone();
    let (_, input, extent) = frame(false);
    let (early, early_report) = renderer.render_to_image(&input).unwrap();
    drop(early);
    assert_eq!(renderer.texture_cache_stats().idle, 0);
    wait(context.device(), Some(early_report.submission));
    assert_eq!(renderer.texture_cache_stats().idle, 1);
    let (output, initial) = renderer.render_to_image(&input).unwrap();
    let texture = output.native_texture().unwrap().clone();
    let imported = Arc::new(output);
    wait(context.device(), Some(initial.submission));
    assert_eq!(renderer.texture_cache_stats().idle, 0);
    let (next, _) = renderer.render_to_image(&input).unwrap();
    assert_ne!(next.native_texture(), Some(&texture));
    let imported_sampler = Arc::clone(&imported);
    let (readback, sampled) = copy_to_readback(context.device(), context.queue(), &texture, extent);
    context
        .queue()
        .on_submitted_work_done(move || drop(imported_sampler));
    drop(imported);
    assert_eq!(renderer.texture_cache_stats().idle, 0);
    assert!(
        read_pixels(context.device(), readback, sampled, extent)
            .iter()
            .all(|pixel| pixel[0].abs_diff(137) <= 1 && pixel[1..] == [0, 0, 64])
    );
    assert_eq!(renderer.texture_cache_stats().idle, 1);
    let (reused, report) = renderer.render_to_image(&input).unwrap();
    assert_eq!(reused.native_texture(), Some(&texture));
    assert_eq!(report.work.created_texture_calls, 0);
    assert_eq!(report.work.created_buffer_calls, 0);
    wait(context.device(), Some(report.submission));
    context.device().destroy();
    assert!(matches!(
        renderer.check_errors(),
        Err(WgpuRenderError::DeviceLost)
    ));
    assert!(next.native_texture().is_none());
    assert!(reused.native_texture().is_none());
}

struct UploadTrackingOverlay {
    clears: Arc<AtomicUsize>,
}

impl RenderPass for UploadTrackingOverlay {
    fn attachments(&self) -> PassAttachments {
        PassAttachments::OVERLAY
    }

    fn execute(&mut self, context: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError> {
        let _queue = context.queue;
        let _buffer = context.upload_buffer(
            "test_editor_overlay_vertices",
            wgpu::BufferUsages::VERTEX,
            &[0u8; 16],
        )?;
        Ok(())
    }

    fn clear_caches(&mut self) {
        self.clears.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn editor_overlay_reuses_uploaded_buffers_and_clears_caches_on_device_loss() {
    let mut renderer = gpu();
    let context = renderer.context().clone();
    let (_, input, _) = frame(false);
    let clears = Arc::new(AtomicUsize::new(0));
    renderer
        .set_editor_overlay(Some(Box::new(UploadTrackingOverlay {
            clears: Arc::clone(&clears),
        })))
        .unwrap();

    let (first_output, first) = renderer.render_to_image(&input).unwrap();
    assert_eq!(first.work.created_buffer_calls, 2);
    drop(first_output);
    wait(context.device(), Some(first.submission));

    let (second_output, second) = renderer.render_to_image(&input).unwrap();
    assert_eq!(second.work.created_buffer_calls, 0);
    assert_eq!(second.work.created_texture_calls, 0);
    drop(second_output);
    wait(context.device(), Some(second.submission));

    assert_eq!(clears.load(Ordering::Relaxed), 0);
    context.device().destroy();
    assert!(matches!(
        renderer.check_errors(),
        Err(WgpuRenderError::DeviceLost)
    ));
    assert_eq!(clears.load(Ordering::Relaxed), 1);
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn offscreen_targets_apply_backpressure_until_a_lease_returns() {
    use super::super::texture_cache::MAX_ACTIVE_TEXTURES;

    let mut renderer = gpu();
    let context = renderer.context().clone();
    let (_, input, _) = frame(false);
    let mut held = Vec::new();
    for _ in 0..MAX_ACTIVE_TEXTURES {
        let (output, _) = renderer.render_to_image(&input).unwrap();
        held.push(output);
    }
    wait(context.device(), None);
    assert!(matches!(
        renderer.render_to_image(&input),
        Err(WgpuRenderError::TargetsBusy)
    ));

    let released = held.pop().unwrap();
    let texture = released.native_texture().unwrap().clone();
    drop(released);
    let (reused, report) = renderer.render_to_image(&input).unwrap();
    assert_eq!(reused.native_texture(), Some(&texture));
    assert_eq!(report.work.created_texture_calls, 0);
}

fn size(extent: Extent2d) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: extent.width,
        height: extent.height,
        depth_or_array_layers: 1,
    }
}

fn wait(device: &wgpu::Device, submission: Option<wgpu::SubmissionIndex>) {
    device
        .poll(wgpu::PollType::Wait {
            submission_index: submission,
            timeout: Some(Duration::from_secs(5)),
        })
        .unwrap();
}

fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    extent: Extent2d,
) -> Vec<[u8; 4]> {
    let (buffer, submission) = copy_to_readback(device, queue, texture, extent);
    read_pixels(device, buffer, submission, extent)
}

fn copy_to_readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    extent: Extent2d,
) -> (wgpu::Buffer, wgpu::SubmissionIndex) {
    let row_stride = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row_stride) * u64::from(extent.height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_stride),
                rows_per_image: None,
            },
        },
        size(extent),
    );
    let submission = queue.submit([encoder.finish()]);
    (buffer, submission)
}

fn read_pixels(
    device: &wgpu::Device,
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    extent: Extent2d,
) -> Vec<[u8; 4]> {
    let (mapped, received) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            mapped.send(result).unwrap()
        });
    wait(device, Some(submission));
    received
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    buffer
        .slice(..)
        .get_mapped_range()
        .unwrap()
        .chunks_exact(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize)
        .flat_map(|row| row[..extent.width as usize * 4].chunks_exact(4))
        .map(|pixel| pixel.try_into().unwrap())
        .collect()
}
