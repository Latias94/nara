use std::{cell::RefCell, rc::Rc, sync::mpsc, time::Duration};

use nara_app::{App, RuntimeInstance};
use nara_core::Mat3;
use nara_ecs::{Entity, World};
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
    RenderImagePacket, RenderTarget, ViewLoadOp, ViewMapping2d, ViewportRect, WorldCanvas,
    build_image_render_frame_packet, build_render_frame_packet_with_world_canvas,
    gpu::{GpuContext, RenderInput, RenderTargetView, Renderer, WgpuImageOutput},
};
use nara_window::{Window, WindowResolution};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

struct PassProbe(Rc<RefCell<EditorOverlayPass>>);

impl RenderPass for PassProbe {
    fn attachments(&self) -> PassAttachments {
        self.0.borrow().attachments()
    }

    fn execute(&mut self, context: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError> {
        self.0.borrow_mut().execute(context)
    }

    fn clear_caches(&mut self) {
        self.0.borrow_mut().clear_caches();
    }
}

fn renderer() -> Renderer {
    let context = GpuContext::request(
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env()),
        None,
    )
    .unwrap();
    eprintln!("{:?}", context.adapter().get_info());
    Renderer::new(context)
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn overlay_composites_antialiased_world_shapes_on_srgb_targets() {
    let mut renderer = renderer();
    let context = renderer.context().clone();
    let (pass, state) = EditorOverlayPass::new();
    let pass = Rc::new(RefCell::new(pass));
    renderer
        .set_editor_overlay(Some(Box::new(PassProbe(pass.clone()))))
        .unwrap();

    let extent = Extent2d::new(128, 96).unwrap();
    let window = Window::new("overlay test", WindowResolution::new(128, 96));
    let canvas = WorldCanvas::new(64, 36)
        .unwrap()
        .with_bar_color(Color::rgb(0.0, 0.0, 1.0))
        .unwrap();
    let target = RenderTarget::Window(window.id);
    let first = view(
        target,
        ViewportRect::from_extent(canvas.extent()).unwrap(),
        ViewLoadOp::Clear(Color::rgba(1.0, 0.0, 0.0, 0.25)),
    );
    let viewport = ViewportRect::new(8, 4, 48, 28).unwrap();
    let camera = Mat3::from_translation(Vec2::new(7.25, -3.0)) * Mat3::from_angle(0.27);
    let selected = ExtractedView {
        order: 1,
        mapping: ViewMapping2d::orthographic(camera, 28.0, viewport).unwrap(),
        ..view(target, viewport, ViewLoadOp::Load)
    };
    let mut views = ExtractedViews::default();
    views.push(first);
    views.push(selected);
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let window_packet = build_render_frame_packet_with_world_canvas(
        runtime.generation(),
        1,
        Some(window.id),
        [&window],
        &views,
        Some(canvas),
    )
    .unwrap()
    .unwrap();
    let window_input = RenderInput::capture(runtime.world(), window_packet.clone());
    let image_input = RenderInput::capture(
        runtime.world(),
        window_packet
            .with_image_target(
                RenderImagePacket::new(RenderImage2d::allocate_handle().unwrap(), extent).unwrap(),
            )
            .unwrap(),
    );
    runtime.finish().unwrap();

    let world = |x: f32, y: f32| {
        selected
            .viewport_to_world(Vec2::new(x, y - 12.0) * 0.5)
            .unwrap()
    };
    let segment = |from: [f32; 2], to: [f32; 2], color, width_px| OverlayPrimitive::Line {
        from: world(from[0], from[1]),
        to: world(to[0], to[1]),
        stroke: OverlayStroke::new(color, width_px),
    };
    let circle = |x, y, radius, color| {
        let center = world(x, y);
        OverlayPrimitive::Circle {
            center,
            radius: center.distance(world(x + radius, y)),
            stroke: OverlayStroke::new(color, 2.0),
        }
    };
    let rect = |corners: [[f32; 2]; 4], color| OverlayPrimitive::Rect {
        corners: corners.map(|[x, y]| world(x, y)),
        stroke: OverlayStroke::new(color, 2.0),
    };
    let red = Color::rgb(1.0, 0.0, 0.0);
    let green = Color::rgb(0.0, 1.0, 0.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let mut overlay = OverlayFrame {
        view: Some(selected.id),
        primitives: vec![
            segment(
                [-8.0, 28.0],
                [100.0, 28.0],
                Color::rgba(0.0, 1.0, 0.0, 0.5),
                3.0,
            ),
            rect(
                [[32.5, 40.5], [50.5, 42.5], [48.5, 56.5], [30.5, 54.5]],
                Color::rgba(0.0, 0.0, 1.0, 0.5),
            ),
            circle(68.5, 50.5, 8.0, blue),
            circle(100.5, 35.5, 7.0, Color::WHITE),
            rect(
                [[94.5, 45.5], [104.5, 45.5], [104.5, 55.5], [94.5, 55.5]],
                green,
            ),
            segment([82.5, 63.5], [82.5, 47.5], green, 2.0),
            segment([97.5, 59.5], [103.5, 63.5], red, 2.0),
            segment([97.5, 67.5], [103.5, 63.5], red, 2.0),
            segment([24.5, 65.5], [36.5, 65.5], green, 0.25),
            segment([82.5, 63.5], [103.5, 63.5], red, 2.0),
        ],
        ..Default::default()
    };
    let producer = state.clone();
    producer.update(overlay.clone());

    let (image, _) = renderer.render_to_image(&image_input).unwrap();
    let pixels = read(&context, image.native_texture().unwrap(), extent);
    let at = |x, y| pixel(&pixels, extent, x, y);
    let scene = [137, 0, 0, 64];
    assert_eq!(at(12, 8), [0, 0, 255, 255]);
    assert_close(at(15, 27), scene);
    assert_close(at(16, 27), [99, 188, 0, 159]);
    assert_close(at(40, 27), [99, 188, 0, 159]);
    assert_close(at(40, 29), [120, 137, 0, 112]);
    assert_close(at(40, 30), scene);
    assert_close(at(32, 40), [99, 0, 188, 159]);
    assert_close(at(40, 48), scene);
    assert_eq!(at(76, 50), [0, 0, 255, 255]);
    assert_close(at(68, 50), scene);
    assert_eq!(at(107, 35), [255, 255, 255, 255]);
    assert_eq!(at(94, 51), [0, 255, 0, 255]);
    assert_close(at(29, 65), [120, 137, 0, 112]);
    assert_eq!(at(92, 63), [255, 0, 0, 255]);

    let format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: size(extent),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let texture_view = texture.create_view(&Default::default());
    let target = RenderTargetView {
        view: &texture_view,
        format,
        extent,
    };
    renderer.render_to_view(&window_input, target).unwrap();
    let mut direct = read(&context, &texture, extent);
    for pixel in &mut direct {
        pixel.swap(0, 2);
    }
    assert_eq!(direct, pixels);
    assert_eq!(pass.borrow().pipelines.len(), 2);
    drop(image);

    overlay.view = None;
    overlay.primitives.pop();
    overlay
        .primitives
        .push(segment([82.5, 63.5], [103.5, 63.5], Color::WHITE, 3.0));
    state.update(overlay.clone());
    let (highlight, _) = renderer.render_to_image(&image_input).unwrap();
    let pixels = read(&context, highlight.native_texture().unwrap(), extent);
    assert_eq!(pixel(&pixels, extent, 92, 63), [255, 255, 255, 255]);
    assert_eq!(pixel(&pixels, extent, 94, 51), [0, 255, 0, 255]);
    drop(highlight);

    overlay.view = Some(RenderViewId::try_acquire().unwrap());
    state.update(overlay);
    let (unmatched, _) = renderer.render_to_image(&image_input).unwrap();
    let pixels = read(&context, unmatched.native_texture().unwrap(), extent);
    assert_close(pixel(&pixels, extent, 92, 63), scene);
    assert_close(pixel(&pixels, extent, 16, 27), scene);
    renderer.check_errors().unwrap();
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn overlay_reuses_host_uploads_and_rebuilds_after_device_loss() {
    let mut initial_renderer = renderer();
    let context = initial_renderer.context().clone();
    let (pass, state) = EditorOverlayPass::new();
    let pass = Rc::new(RefCell::new(pass));
    initial_renderer
        .set_editor_overlay(Some(Box::new(PassProbe(pass.clone()))))
        .unwrap();

    let extent = Extent2d::new(64, 64).unwrap();
    let image = RenderImage2d::allocate_handle().unwrap();
    let mut views = ExtractedViews::default();
    views.push(view(
        RenderTarget::Image(image),
        ViewportRect::from_extent(extent).unwrap(),
        ViewLoadOp::Clear(Color::BLACK),
    ));
    let packet = build_image_render_frame_packet(
        RenderFrameSourceGeneration::try_acquire().unwrap(),
        1,
        &views,
        RenderImagePacket::new(image, extent).unwrap(),
    )
    .unwrap()
    .unwrap();
    let input = RenderInput::capture(&World::new(), packet);
    let overlay = OverlayFrame {
        grid: Some(OverlayGrid {
            origin: Vec2::new(-15.5, -16.5),
            spacing: 16.0,
            line: OverlayStroke::new(Color::WHITE, 1.0),
            x_axis: OverlayStroke::new(Color::rgb(1.0, 0.0, 0.0), 1.0),
            y_axis: OverlayStroke::new(Color::rgb(0.0, 1.0, 0.0), 1.0),
        }),
        primitives: vec![OverlayPrimitive::Line {
            from: Vec2::new(-23.5, 23.5),
            to: Vec2::new(-7.5, 23.5),
            stroke: OverlayStroke::new(Color::rgb(0.0, 1.0, 1.0), 2.0),
        }],
        ..Default::default()
    };
    state.update(overlay);

    let (output, first) = initial_renderer.render_to_image(&input).unwrap();
    assert_eq!(first.work.created_buffer_calls, 2);
    assert_eq!(first.work.created_texture_calls, 1);
    assert_eq!(first.work.issued_buffer_write_calls, 1);
    assert_eq!(first.work.issued_texture_write_calls, 0);
    let pipeline = pass.borrow().pipelines[&WgpuImageOutput::FORMAT].clone();
    let texture = output.native_texture().unwrap().clone();
    let expected = read(&context, &texture, extent);
    assert_eq!(pixel(&expected, extent, 20, 48), [255, 0, 0, 255]);
    assert_eq!(pixel(&expected, extent, 16, 44), [0, 255, 0, 255]);
    assert_eq!(pixel(&expected, extent, 20, 32), [255, 255, 255, 255]);
    assert_eq!(pixel(&expected, extent, 22, 44), [0, 0, 0, 255]);
    assert_eq!(pixel(&expected, extent, 16, 8), [0, 255, 255, 255]);
    drop(output);

    let (output, stable) = initial_renderer.render_to_image(&input).unwrap();
    assert_eq!(stable.work.created_buffer_calls, 0);
    assert_eq!(stable.work.created_texture_calls, 0);
    assert_eq!(stable.work.issued_buffer_write_calls, 1);
    assert_eq!(output.native_texture(), Some(&texture));
    assert_eq!(pass.borrow().pipelines[&WgpuImageOutput::FORMAT], pipeline);
    assert_eq!(read(&context, &texture, extent), expected);
    drop((texture, pipeline));

    context.device().destroy();
    assert!(matches!(
        initial_renderer.check_errors(),
        Err(WgpuRenderError::DeviceLost)
    ));
    assert!(pass.borrow().pipelines.is_empty());
    assert!(pass.borrow().device.is_none());
    assert!(output.native_texture().is_none());
    drop((initial_renderer, output, context));

    let mut recovered = renderer();
    recovered
        .set_editor_overlay(Some(Box::new(PassProbe(pass.clone()))))
        .unwrap();
    let (output, rebuilt) = recovered.render_to_image(&input).unwrap();
    assert_eq!(pass.borrow().pipelines.len(), 1);
    assert_eq!(
        pass.borrow().device.as_ref(),
        Some(recovered.context().device())
    );
    assert_eq!(rebuilt.work.created_buffer_calls, 2);
    assert_eq!(
        read(
            recovered.context(),
            output.native_texture().unwrap(),
            extent
        ),
        expected
    );
    drop(output);

    state.update(OverlayFrame::default());
    let (empty, report) = recovered.render_to_image(&input).unwrap();
    assert_eq!(report.work.issued_buffer_write_calls, 0);
    let pixels = read(recovered.context(), empty.native_texture().unwrap(), extent);
    assert!(pixels.into_iter().all(|pixel| pixel == [0, 0, 0, 255]));
    recovered.check_errors().unwrap();
}

fn view(target: RenderTarget, viewport: ViewportRect, load: ViewLoadOp) -> ExtractedView {
    ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target,
        viewport,
        mapping: ViewMapping2d::orthographic(
            Mat3::IDENTITY,
            viewport.physical_height as f32,
            viewport,
        )
        .unwrap(),
        order: 0,
        load,
    }
}

fn size(extent: Extent2d) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: extent.width,
        height: extent.height,
        depth_or_array_layers: 1,
    }
}

fn read(context: &GpuContext, texture: &wgpu::Texture, extent: Extent2d) -> Vec<[u8; 4]> {
    let device = context.device();
    let row_bytes = extent.width * 4;
    let stride = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(stride) * u64::from(extent.height),
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
                bytes_per_row: Some(stride),
                rows_per_image: None,
            },
        },
        size(extent),
    );
    let submission = context.queue().submit([encoder.finish()]);
    let (mapped, received) = mpsc::channel();
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, move |result| {
        mapped.send(result).unwrap()
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(WAIT),
        })
        .unwrap();
    received.recv_timeout(WAIT).unwrap().unwrap();
    slice
        .get_mapped_range()
        .unwrap()
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..row_bytes as usize].chunks_exact(4))
        .map(|pixel| pixel.try_into().unwrap())
        .collect()
}

fn pixel(pixels: &[[u8; 4]], extent: Extent2d, x: u32, y: u32) -> [u8; 4] {
    pixels[(y * extent.width + x) as usize]
}

fn assert_close(actual: [u8; 4], expected: [u8; 4]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 1),
        "pixel {actual:?} differs from {expected:?}"
    );
}
