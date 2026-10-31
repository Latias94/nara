//! A normal Feature that copies declared resources into one bounded private MAP_READ buffer.

use std::{
    sync::mpsc::{self, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use nara_render_wgpu::{
    WgpuFeatureAllocation, WgpuFeatureEncodeContext, WgpuFeaturePrepareContext, WgpuProgramTexture,
    WgpuRenderFeature, WgpuRenderProgramError,
};

use super::{model::*, workload::Ports};

const READBACK_BYTES: u64 = 4 * IMAGE_BYTES + 2 * COUNTS_BYTES;

pub struct Readback;

pub struct ReadbackPrepared {
    ports: Ports,
    report: ReadbackReport,
}

#[derive(Clone)]
pub struct ReadbackReport {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

pub struct Sample {
    pub scene: Vec<[f32; 4]>,
    pub blurred: Vec<[f32; 4]>,
    pub effect: Vec<[f32; 4]>,
    pub histogram: Vec<[f32; 4]>,
    pub raw: [u32; 4],
    pub frequencies: [f32; 4],
}

impl WgpuRenderFeature for Readback {
    type Capture = Ports;
    type State = ();
    type Prepared = ReadbackPrepared;
    type Report = ReadbackReport;

    fn create_state(
        &self,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn prepare(
        &self,
        _state: &mut (),
        ports: &Ports,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ReadbackPrepared, WgpuRenderProgramError> {
        assert!(READBACK_BYTES < 1024 * 1024);
        assert_eq!((WIDTH * 16) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
        let allocation = context.reserve_bytes(READBACK_BYTES)?;
        let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("public-program-private-readback"),
            size: READBACK_BYTES,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(ReadbackPrepared {
            ports: ports.clone(),
            report: ReadbackReport {
                device: context.device().clone(),
                buffer,
                _allocation: allocation,
            },
        })
    }

    fn encode(
        &self,
        prepared: &ReadbackPrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<ReadbackReport, WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let buffer = &prepared.report.buffer;
        copy_texture(resources, encoder, &prepared.ports.scene, buffer, 0)?;
        copy_texture(
            resources,
            encoder,
            &prepared.ports.blurred,
            buffer,
            IMAGE_BYTES,
        )?;
        copy_texture(
            resources,
            encoder,
            &prepared.ports.effect,
            buffer,
            2 * IMAGE_BYTES,
        )?;
        copy_texture(
            resources,
            encoder,
            &prepared.ports.histogram,
            buffer,
            3 * IMAGE_BYTES,
        )?;
        encoder.copy_buffer_to_buffer(
            resources.buffer(&prepared.ports.raw)?,
            0,
            buffer,
            4 * IMAGE_BYTES,
            COUNTS_BYTES,
        );
        encoder.copy_buffer_to_buffer(
            resources.buffer(&prepared.ports.frequencies)?,
            0,
            buffer,
            4 * IMAGE_BYTES + COUNTS_BYTES,
            COUNTS_BYTES,
        );
        Ok(prepared.report.clone())
    }
}

fn copy_texture<S: 'static>(
    resources: nara_render_wgpu::WgpuOperationResources<'_>,
    encoder: &mut wgpu::CommandEncoder,
    port: &WgpuProgramTexture<S>,
    buffer: &wgpu::Buffer,
    offset: u64,
) -> Result<(), WgpuRenderProgramError> {
    encoder.copy_texture_to_buffer(
        resources.texture_native(port)?.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset,
                bytes_per_row: Some(WIDTH * 16),
                rows_per_image: Some(HEIGHT),
            },
        },
        extent(),
    );
    Ok(())
}

impl ReadbackReport {
    // Call only after the enclosing Host submission returned its complete successful group.
    pub fn read(&self) -> Sample {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.device
                .poll(wgpu::PollType::Poll)
                .expect("poll the original admitted readback device");
            match receiver.try_recv() {
                Ok(result) => {
                    result.expect("native readback mapping must succeed");
                    break;
                }
                Err(TryRecvError::Empty) => {
                    assert!(
                        Instant::now() < deadline,
                        "native readback mapping timed out"
                    );
                    thread::sleep(Duration::from_millis(1));
                }
                Err(TryRecvError::Disconnected) => panic!("native readback callback disappeared"),
            }
        }
        let sample = {
            let mapped = self
                .buffer
                .get_mapped_range(..)
                .expect("the readback buffer is mapped");
            assert_eq!(mapped.len() as u64, READBACK_BYTES);
            let image = |index: usize| -> Vec<[f32; 4]> {
                mapped[index * IMAGE_BYTES as usize..(index + 1) * IMAGE_BYTES as usize]
                    .chunks_exact(16)
                    .map(|pixel| {
                        std::array::from_fn(|channel| {
                            f32::from_le_bytes(
                                pixel[channel * 4..channel * 4 + 4].try_into().unwrap(),
                            )
                        })
                    })
                    .collect()
            };
            let words = |offset: usize| -> [u32; 4] {
                std::array::from_fn(|index| {
                    let start = offset + index * 4;
                    u32::from_le_bytes(mapped[start..start + 4].try_into().unwrap())
                })
            };
            Sample {
                scene: image(0),
                blurred: image(1),
                effect: image(2),
                histogram: image(3),
                raw: words(4 * IMAGE_BYTES as usize),
                frequencies: words((4 * IMAGE_BYTES + COUNTS_BYTES) as usize).map(f32::from_bits),
            }
        };
        self.buffer.unmap();
        sample
    }
}
