//! Public target admission, exact output readback and finite owner retirement.

use std::{
    sync::mpsc::{self, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use nara::core::{Color, Mat3};
use nara_ecs::Entity;
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFramePacket, RenderFrameSourceGeneration,
    RenderImage2d, RenderImagePacket, RenderTarget, RenderViewId, ViewLoadOp, ViewMapping2d,
    ViewportRect, build_image_render_frame_packet,
};
use nara_render_wgpu::{WgpuImageOutput, WgpuRenderHost, WgpuRenderProducer, WgpuSubmissionPrefix};

use super::heatmap::{GRID_SIDE, HeatmapCapture};

pub const NATIVE_TIMEOUT: Duration = Duration::from_secs(10);

pub fn require_native_selection() {
    let backend = std::env::var("WGPU_BACKEND").expect("explicitly select WGPU_BACKEND");
    assert!(
        matches!(backend.as_str(), "dx12" | "vulkan" | "metal"),
        "select exactly one native backend: dx12, vulkan or metal",
    );
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0"),
        "native qualification requires NARA_WGPU_FORCE_FALLBACK=0",
    );
}

pub fn source() -> RenderFrameSourceGeneration {
    RenderFrameSourceGeneration::try_acquire().expect("a non-reused independent source")
}

pub fn target(width: u32, height: u32) -> RenderImagePacket {
    RenderImagePacket::new(
        RenderImage2d::allocate_handle().expect("an independent logical image"),
        Extent2d::new(width, height).expect("a nonempty extent"),
    )
    .expect("a valid image target")
}

pub fn packet(
    source: RenderFrameSourceGeneration,
    view_id: RenderViewId,
    frame: u64,
    target: RenderImagePacket,
) -> RenderFramePacket {
    let viewport = ViewportRect::from_extent(target.extent()).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: view_id,
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Image(target.image()),
        viewport,
        mapping: ViewMapping2d::orthographic(
            Mat3::IDENTITY,
            target.extent().height as f32,
            viewport,
        )
        .unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    build_image_render_frame_packet(source, frame, &views, target)
        .expect("public neutral topology must validate")
        .expect("one View produces a packet")
}

pub fn admit(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    source: RenderFrameSourceGeneration,
    target: RenderImagePacket,
) {
    let reservation = host.reserve_image_target(producer, source, target).unwrap();
    host.activate_image_target(producer, reservation).unwrap();
}

pub fn settle(prefix: &WgpuSubmissionPrefix) {
    let deadline = Instant::now() + NATIVE_TIMEOUT;
    while !prefix.poll().expect("poll the original submission prefix") {
        assert!(Instant::now() < deadline, "original GPU prefix timed out");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(prefix.is_complete());
}

pub fn finish(host: &mut WgpuRenderHost) {
    let deadline = Instant::now() + NATIVE_TIMEOUT;
    while !host
        .poll_retirement()
        .expect("poll the original Host retirement")
    {
        assert!(Instant::now() < deadline, "Host retirement timed out");
        thread::sleep(Duration::from_millis(1));
    }
}

pub fn assert_heatmap(
    host: &mut WgpuRenderHost,
    output: &WgpuImageOutput,
    capture: &HeatmapCapture,
    palette_shift: u32,
) {
    assert_eq!(output.format(), wgpu::TextureFormat::Rgba8Unorm);
    let extent = output.extent();
    let row_bytes = extent.width * 4;
    let padded_row =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let bytes = u64::from(padded_row) * u64::from(extent.height);
    assert!(bytes > 0 && bytes <= 4 * 1024 * 1024);
    let (buffer, prefix) = host
        .submit_image_use(
            std::slice::from_ref(output),
            |device, _queue, encoder, views| {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("independent-policy-readback"),
                    size: bytes,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    views[0].texture().as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(padded_row),
                            rows_per_image: Some(extent.height),
                        },
                    },
                    wgpu::Extent3d {
                        width: extent.width,
                        height: extent.height,
                        depth_or_array_layers: 1,
                    },
                );
                Ok(buffer)
            },
        )
        .expect("the production Host must admit the exact output use");
    let (sender, receiver) = mpsc::sync_channel(1);
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + NATIVE_TIMEOUT;
    loop {
        prefix.poll().expect("poll original readback custody");
        host.with_image(output, |device, _, _| device.poll(wgpu::PollType::Poll))
            .expect("poll the exact output owner")
            .expect("service the original map callback");
        match receiver.try_recv() {
            Ok(result) => {
                result.expect("native mapping must succeed");
                break;
            }
            Err(TryRecvError::Empty) => {
                assert!(Instant::now() < deadline, "readback mapping timed out");
                thread::sleep(Duration::from_millis(1));
            }
            Err(TryRecvError::Disconnected) => panic!("the map callback disappeared"),
        }
    }
    settle(&prefix);
    {
        let mapped = buffer
            .get_mapped_range(..)
            .expect("the output copy is mapped");
        assert_eq!(mapped.len() as u64, bytes);
        let palette: [[u8; 4]; 4] = [
            [0, 0, 255, 255],
            [0, 255, 255, 255],
            [255, 255, 0, 255],
            [255, 0, 0, 255],
        ];
        for (y, row) in mapped.chunks_exact(padded_row as usize).enumerate() {
            for (x, pixel) in row[..row_bytes as usize].chunks_exact(4).enumerate() {
                let column = x * GRID_SIDE as usize / extent.width as usize;
                let row = y * GRID_SIDE as usize / extent.height as usize;
                let sample = capture.samples[row * GRID_SIDE as usize + column];
                let expected = palette[((sample + palette_shift) % GRID_SIDE) as usize];
                assert_eq!(
                    pixel,
                    expected.as_slice(),
                    "exact heatmap pixel at ({x}, {y})"
                );
            }
        }
    }
    buffer.unmap();
}
