//! Exact-wgpu private measurement harness. No production GPU/target authority is exposed.

use std::{sync::mpsc, time::Duration};

const MAX_READBACK_BYTES: u64 = 16 * 1024 * 1024;

pub(super) struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub fn new() -> Self {
        let backends = if cfg!(windows) {
            wgpu::Backends::DX12
        } else {
            wgpu::Backends::VULKAN
        };
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = backends;
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .expect("explicit render comparison requires an available native GPU adapter");
        let info = adapter.get_info();
        eprintln!(
            "render-comparison adapter={:?} backend={:?} device={:?}",
            info.name, info.backend, info.device_type
        );
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("nara-render-composition-comparison"),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .expect("render comparison requires the selected native device");
        Self { device, queue }
    }

    pub fn rgba32_texture(
        &self,
        label: &str,
        width: u32,
        height: u32,
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        let bytes = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(16))
            .unwrap();
        assert!(width > 0 && height > 0 && bytes <= MAX_READBACK_BYTES);
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage,
            view_formats: &[],
        })
    }

    pub fn read_rgba32(&self, texture: &wgpu::Texture, width: u32, height: u32) -> Vec<[f32; 4]> {
        let row = width.checked_mul(16).unwrap();
        let padded_row = row
            .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            .checked_mul(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            .unwrap();
        let bytes = u64::from(padded_row)
            .checked_mul(u64::from(height))
            .unwrap();
        assert!(width > 0 && height > 0 && bytes <= MAX_READBACK_BYTES);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render-comparison-readback"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-comparison-readback"),
            });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.queue.submit([encoder.finish()]);
        let encoded = self.map_after_submission(&readback, submission);
        let mut output = Vec::with_capacity((width as usize).checked_mul(height as usize).unwrap());
        for row_bytes in encoded.chunks_exact(padded_row as usize) {
            for pixel in row_bytes[..row as usize].chunks_exact(16) {
                output.push(std::array::from_fn(|index| {
                    f32::from_le_bytes(pixel[index * 4..index * 4 + 4].try_into().unwrap())
                }));
            }
        }
        output
    }

    pub fn upload_rgba32(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
        pixels: &[[f32; 4]],
    ) {
        let count = (width as usize).checked_mul(height as usize).unwrap();
        assert_eq!(pixels.len(), count);
        let bytes = count.checked_mul(16).unwrap();
        assert!(bytes as u64 <= MAX_READBACK_BYTES);
        let mut encoded = Vec::with_capacity(bytes);
        for pixel in pixels {
            for value in pixel {
                encoded.extend_from_slice(&value.to_le_bytes());
            }
        }
        self.queue.write_texture(
            texture.as_image_copy(),
            &encoded,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width.checked_mul(16).unwrap()),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn read_buffer(&self, buffer: &wgpu::Buffer) -> Vec<u8> {
        let bytes = buffer.size();
        assert!(bytes > 0 && bytes <= MAX_READBACK_BYTES);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render-comparison-buffer-readback"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-comparison-buffer-readback"),
            });
        encoder.copy_buffer_to_buffer(buffer, 0, &readback, 0, bytes);
        let submission = self.queue.submit([encoder.finish()]);
        self.map_after_submission(&readback, submission)
    }

    pub fn map_after_submission(
        &self,
        buffer: &wgpu::Buffer,
        submission: wgpu::SubmissionIndex,
    ) -> Vec<u8> {
        let (sender, receiver) = mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("render comparison GPU completion timeout");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("readback callback timeout")
            .expect("readback mapping failed");
        let mapped = buffer.get_mapped_range(..).unwrap();
        let bytes = mapped.to_vec();
        drop(mapped);
        buffer.unmap();
        bytes
    }
}
