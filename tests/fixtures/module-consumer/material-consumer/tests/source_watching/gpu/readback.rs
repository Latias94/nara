use std::{
    sync::mpsc::{self, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use nara::render_wgpu::{TextureInterpretation, TextureTransfer, WgpuImageOutput, WgpuRenderHost};

const WIDTH: u32 = 32;
const HEIGHT: u32 = 24;
const ROW_STRIDE: u32 = 256;

/// Reads the exact published linear RGBA8 image in row-major order without row padding.
/// Requires both the Host's original copy prefix and the map callback within ten seconds.
pub(super) fn read(host: &mut WgpuRenderHost, image: &WgpuImageOutput) -> Vec<[u8; 4]> {
    let extent = wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };
    let descriptor = image.descriptor();
    assert_eq!(
        (image.extent().width, image.extent().height),
        (WIDTH, HEIGHT)
    );
    assert_eq!(descriptor.native.size, extent);
    assert_eq!(descriptor.native.format, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(image.format(), wgpu::TextureFormat::Rgba8Unorm);
    let TextureInterpretation::Color { transfer, .. } = descriptor.interpretation else {
        panic!("expected a color texture");
    };
    assert_eq!(transfer, TextureTransfer::Linear);
    assert!(
        descriptor
            .native
            .usage
            .contains(wgpu::TextureUsages::COPY_SRC)
    );
    assert_eq!(ROW_STRIDE % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);

    let ((device, readback), prefix) = host
        .submit_image_use(std::slice::from_ref(image), |device, _, encoder, views| {
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("material-source-watching-readback"),
                size: u64::from(ROW_STRIDE) * u64::from(HEIGHT),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                views[0].texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(ROW_STRIDE),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                extent,
            );
            Ok((device.clone(), readback))
        })
        .expect("the Host must submit the exact published image copy");

    let deadline = Instant::now() + Duration::from_secs(10);
    let (sender, receiver) = mpsc::sync_channel(1);
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let mut mapped = false;
    loop {
        assert!(
            Instant::now() < deadline,
            "readback prefix and mapping did not complete within ten seconds"
        );
        device
            .poll(wgpu::PollType::Poll)
            .expect("the original device must service the readback map callback");
        let completed = prefix
            .poll()
            .expect("the original readback prefix must remain pollable");
        if !mapped {
            match receiver.try_recv() {
                Ok(result) => {
                    result.expect("readback mapping must succeed");
                    mapped = true;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => panic!("readback map callback disappeared"),
            }
        }
        if completed && mapped {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        prefix.is_complete(),
        "the actual readback prefix must complete"
    );

    let pixels: Vec<[u8; 4]> = {
        let bytes = readback
            .get_mapped_range(..)
            .expect("readback must be mapped");
        let rows = bytes.chunks_exact(ROW_STRIDE as usize);
        assert!(
            rows.remainder().is_empty(),
            "readback must contain whole rows"
        );
        assert_eq!(
            rows.len(),
            HEIGHT as usize,
            "readback must contain every row"
        );
        rows.flat_map(|row| {
            row[..(WIDTH * 4) as usize]
                .chunks_exact(4)
                .map(|pixel| pixel.try_into().expect("each RGBA8 pixel has four bytes"))
        })
        .collect()
    };
    readback.unmap();
    assert_eq!(pixels.len(), (WIDTH * HEIGHT) as usize);
    pixels
}
