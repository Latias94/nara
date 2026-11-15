use std::{
    sync::mpsc::{self, TryRecvError},
    thread,
    time::Instant,
};

use nara_render_wgpu::{TextureInterpretation, TextureTransfer, WgpuImageOutput, WgpuRenderHost};

use crate::support::image::{NATIVE_TIMEOUT, settle};

pub fn read(host: &mut WgpuRenderHost, image: &WgpuImageOutput) -> Vec<[u8; 4]> {
    let extent = image.extent();
    let row_bytes = extent.width * 4;
    let row_stride =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let size = u64::from(row_stride) * u64::from(extent.height);
    assert!(size > 0 && size <= 4 * 1024 * 1024);
    assert_eq!(image.format(), wgpu::TextureFormat::Rgba8Unorm);
    let TextureInterpretation::Color { transfer, .. } = image.descriptor().interpretation else {
        panic!("expected a color texture");
    };
    assert_eq!(transfer, TextureTransfer::Linear);
    assert!(
        image
            .descriptor()
            .native
            .usage
            .contains(wgpu::TextureUsages::COPY_SRC)
    );
    let ((device, buffer), prefix) = host
        .submit_image_use(std::slice::from_ref(image), |device, _, encoder, views| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ordinary-material-captured-time-readback"),
                size,
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
                        rows_per_image: Some(extent.height),
                    },
                },
                wgpu::Extent3d {
                    width: extent.width,
                    height: extent.height,
                    depth_or_array_layers: 1,
                },
            );
            Ok((device.clone(), buffer))
        })
        .expect("the original Host must submit the exact image copy");
    let (sender, receiver) = mpsc::sync_channel(1);
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + NATIVE_TIMEOUT;
    let mut mapped = false;
    loop {
        assert!(
            Instant::now() < deadline,
            "original image readback timed out"
        );
        device
            .poll(wgpu::PollType::Poll)
            .expect("poll the original mapping device");
        let complete = prefix.poll().expect("poll the original image-copy prefix");
        if !mapped {
            match receiver.try_recv() {
                Ok(result) => {
                    result.expect("the original readback must map");
                    mapped = true;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => panic!("the readback callback disappeared"),
            }
        }
        if complete && mapped {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(1));
    }
    settle(&prefix);
    let pixels = {
        let bytes = buffer
            .get_mapped_range(..)
            .expect("the image copy is mapped");
        assert_eq!(bytes.len() as u64, size);
        bytes
            .chunks_exact(row_stride as usize)
            .flat_map(|row| {
                row[..row_bytes as usize]
                    .chunks_exact(4)
                    .map(|pixel| pixel.try_into().unwrap())
            })
            .collect::<Vec<[u8; 4]>>()
    };
    buffer.unmap();
    assert_eq!(pixels.len(), (extent.width * extent.height) as usize);
    pixels
}
