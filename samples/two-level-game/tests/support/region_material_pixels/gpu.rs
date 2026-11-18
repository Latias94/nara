use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use nara::{
    core::Vec2,
    render::{Extent2d, RenderImage2d, RenderImagePacket, ViewportRect},
    render_wgpu::{
        TextureAlpha, TextureInterpretation, TextureTransfer, WgpuImageOutput, WgpuRenderHost,
        WgpuRenderInput, WgpuRenderProducer, WgpuStockPolicy,
    },
};

pub const EXTENT: Extent2d = Extent2d {
    width: 960,
    height: 540,
};
const TIMEOUT: Duration = Duration::from_secs(10);

pub fn draw(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    original: &WgpuRenderInput,
    host_frame: &mut u64,
) -> Vec<u8> {
    assert_eq!(original.packet().target().extent(), EXTENT);
    assert!(original.packet().world_canvas().is_none());
    let [view] = original.packet().views() else {
        panic!("the ordinary game must retain its one original camera View");
    };
    assert_eq!(view.viewport, ViewportRect::from_extent(EXTENT).unwrap());
    for (world, physical) in [
        (Vec2::ZERO, Vec2::new(480.0, 270.0)),
        (Vec2::new(-120.0, 48.0), Vec2::new(300.0, 198.0)),
        (Vec2::new(-120.0, -48.0), Vec2::new(300.0, 342.0)),
    ] {
        assert!(view.world_to_viewport(world).abs_diff_eq(physical, 0.0001));
    }
    *host_frame = host_frame.checked_add(1).unwrap();
    host.begin_frame(*host_frame).unwrap();
    let target = RenderImagePacket::new(RenderImage2d::allocate_handle().unwrap(), EXTENT).unwrap();
    let input = original.with_image_target(target).unwrap();
    assert_eq!(input.installation(), original.installation());
    assert_eq!(
        input.packet().source_generation(),
        original.packet().source_generation()
    );
    assert_eq!(
        input.packet().frame_index(),
        original.packet().frame_index()
    );
    assert_eq!(input.packet().views(), original.packet().views());
    assert_eq!(
        input.packet().world_canvas(),
        original.packet().world_canvas()
    );
    assert!(std::ptr::eq(
        input.captured::<WgpuStockPolicy>().unwrap(),
        original.captured::<WgpuStockPolicy>().unwrap(),
    ));
    let reservation = host
        .reserve_image_target(producer, input.packet().source_generation(), target)
        .unwrap();
    host.activate_image_target(producer, reservation).unwrap();
    let candidate = host.prepare(producer, &input).unwrap();
    let published = host.submit_candidate(producer, candidate).unwrap();
    let image = published.image();
    let receipt = image.receipt();
    assert_eq!(receipt.producer_id(), producer.id());
    assert_eq!(receipt.target(), target);
    assert_eq!(
        receipt.source_generation(),
        original.packet().source_generation()
    );
    assert_eq!(
        receipt.render_transaction_frame_index(),
        original.packet().frame_index()
    );
    assert_eq!(receipt.backend_instance_id(), host.backend_instance_id());
    assert_eq!(receipt.device_epoch(), host.device_epoch());
    assert!(receipt.output_version() > 0 && receipt.target_generation() > 0);
    let adapter = host
        .adapter_summary()
        .expect("an actual selected DX12 adapter");
    assert_eq!(adapter.backend(), "dx12");
    assert!(matches!(
        adapter.device_type(),
        "discrete-gpu" | "integrated-gpu"
    ));
    readback(host, image)
}

fn readback(host: &mut WgpuRenderHost, image: &WgpuImageOutput) -> Vec<u8> {
    assert_eq!(image.extent(), EXTENT);
    assert_eq!(image.format(), wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(
        image.descriptor().interpretation,
        TextureInterpretation::Color {
            transfer: TextureTransfer::Linear,
            alpha: TextureAlpha::Premultiplied,
        },
    );
    let row_bytes = EXTENT.width * 4;
    let row_stride =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let (buffer, prefix) = host
        .submit_image_use(std::slice::from_ref(image), |device, _, encoder, views| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("two-level-region-material-original-packet-readback"),
                size: u64::from(row_stride) * u64::from(EXTENT.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                views[0].texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row_stride),
                        rows_per_image: Some(EXTENT.height),
                    },
                },
                wgpu::Extent3d {
                    width: EXTENT.width,
                    height: EXTENT.height,
                    depth_or_array_layers: 1,
                },
            );
            Ok(buffer)
        })
        .unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + TIMEOUT;
    let mut mapped = false;
    loop {
        let completed = prefix.poll().unwrap();
        host.with_image(image, |device, _, _| device.poll(wgpu::PollType::Poll))
            .unwrap()
            .unwrap();
        if !mapped {
            match receiver.try_recv() {
                Ok(result) => {
                    result.unwrap();
                    mapped = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("original readback map callback disappeared")
                }
            }
        }
        if mapped && completed {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original region Material readback exceeded ten seconds"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let pixels = {
        let mapped = buffer.get_mapped_range(..).unwrap();
        assert_eq!(mapped.len(), row_stride as usize * EXTENT.height as usize);
        mapped
            .chunks_exact(row_stride as usize)
            .flat_map(|row| row[..row_bytes as usize].iter().copied())
            .collect::<Vec<_>>()
    };
    buffer.unmap();
    pixels
}

pub fn retire(host: &mut WgpuRenderHost) -> Result<(), String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match host.poll_retirement() {
                Ok(true) => break,
                Err(error) => return Err(error.to_string()),
                Ok(false) if Instant::now() >= deadline => {
                    return Err(
                        "original region Material Host retirement exceeded ten seconds".into(),
                    );
                }
                Ok(false) => thread::sleep(Duration::from_millis(1)),
            }
        }
        if host.retained_resource_bytes() != 0
            || host.retained_candidate_metadata_bytes() != 0
            || host.cleanup_fault_count() != 0
        {
            return Err(
                "original region Material Host retained resources or cleanup faults".into(),
            );
        }
        Ok(())
    }))
    .unwrap_or_else(|_| Err("original region Material Host retirement panicked".into()))
}
