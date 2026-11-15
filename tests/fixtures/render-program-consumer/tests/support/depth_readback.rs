use std::{
    sync::mpsc::{self, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use nara_render_wgpu::{
    WgpuFeatureAllocation, WgpuFeatureEncodeContext, WgpuFeaturePrepareContext, WgpuProgramTexture,
    WgpuRenderFeature, WgpuRenderProgramError,
};

use crate::depth_model::*;

const READBACK_BYTES: u64 = 5 * COLOR_BYTES;

pub struct Readback;

#[derive(Clone)]
pub struct ReadbackCapture {
    pub color: [WgpuProgramTexture<Color>; 2],
    pub depth: [WgpuProgramTexture<ResolvedDepth>; 3],
}

pub struct ReadbackPrepared {
    capture: ReadbackCapture,
    report: ReadbackReport,
}

#[derive(Clone)]
pub struct ReadbackReport {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Observation {
    pub color: [Vec<u8>; 2],
    pub resolved_depth: [Vec<u8>; 3],
}

impl WgpuRenderFeature for Readback {
    type Capture = ReadbackCapture;
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
        capture: &ReadbackCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<ReadbackPrepared, WgpuRenderProgramError> {
        let allocation = context.reserve_bytes(READBACK_BYTES)?;
        let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("independent-depth-original-encoder-readback"),
            size: READBACK_BYTES,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(ReadbackPrepared {
            capture: capture.clone(),
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
        let texture_sources = [
            resources.texture_native(&prepared.capture.color[0])?,
            resources.texture_native(&prepared.capture.color[1])?,
            resources.texture_native(&prepared.capture.depth[0])?,
            resources.texture_native(&prepared.capture.depth[1])?,
            resources.texture_native(&prepared.capture.depth[2])?,
        ];
        for (index, texture) in texture_sources.into_iter().enumerate() {
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &prepared.report.buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: index as u64 * COLOR_BYTES,
                        bytes_per_row: Some(WIDTH * 16),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                extent(),
            );
        }
        Ok(prepared.report.clone())
    }
}

struct Unmap<'a>(&'a wgpu::Buffer);

impl Drop for Unmap<'_> {
    fn drop(&mut self) {
        self.0.unmap();
    }
}

impl ReadbackReport {
    pub fn read(&self) -> Observation {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
        let _unmap = Unmap(&self.buffer);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.device
                .poll(wgpu::PollType::Poll)
                .expect("poll the original Feature device");
            match receiver.try_recv() {
                Ok(result) => {
                    result.expect("map the original submitted color/resolve buffer");
                    break;
                }
                Err(TryRecvError::Empty) => {
                    assert!(
                        Instant::now() < deadline,
                        "depth readback mapping timed out"
                    );
                    thread::sleep(Duration::from_millis(1));
                }
                Err(TryRecvError::Disconnected) => panic!("depth readback callback disappeared"),
            }
        }
        let mapped = self
            .buffer
            .get_mapped_range(..)
            .expect("completed readback mapping");
        assert_eq!(mapped.len() as u64, READBACK_BYTES);
        let image = |index: usize| {
            mapped[index * COLOR_BYTES as usize..(index + 1) * COLOR_BYTES as usize].to_vec()
        };
        Observation {
            color: [image(0), image(1)],
            resolved_depth: [image(2), image(3), image(4)],
        }
    }
}

fn pixels(bytes: &[u8]) -> impl Iterator<Item = [f64; 4]> + '_ {
    assert_eq!(bytes.len() as u64, COLOR_BYTES);
    bytes.chunks_exact(16).map(|pixel| {
        std::array::from_fn(|channel| {
            let start = channel * 4;
            f64::from(f32::from_le_bytes(
                pixel[start..start + 4].try_into().unwrap(),
            ))
        })
    })
}

fn assert_channel(actual: f64, expected: f64, image: &str, pixel: usize, channel: usize) {
    assert!(
        actual.is_finite()
            && (0.0..=1.0).contains(&actual)
            && (actual - expected).abs() <= TOLERANCE,
        "{image} pixel ({}, {}) channel {channel}: actual={actual}, expected={expected}, tolerance={TOLERANCE}",
        pixel % WIDTH as usize,
        pixel / WIDTH as usize,
    );
}

impl Observation {
    pub fn assert_reference(&self, scene: &Scene) {
        let original = reference(scene);
        let preserved = preserved_reference(scene);
        for (bytes, expected) in self.color.iter().zip([&original, &preserved]) {
            for (pixel, (actual, expected)) in pixels(bytes).zip(&expected.color).enumerate() {
                for channel in 0..4 {
                    assert_channel(actual[channel], expected[channel], "color", pixel, channel);
                }
            }
        }
        assert_ne!(
            self.color[0], self.color[1],
            "the second raster pass changes preserved color"
        );
        for (bytes, expected) in self
            .resolved_depth
            .iter()
            .zip([&preserved, &original, &preserved])
        {
            for (pixel, (actual, expected)) in pixels(bytes).zip(&expected.depth).enumerate() {
                for (channel, value) in actual.into_iter().enumerate() {
                    assert_channel(
                        value,
                        if channel == 3 { 1.0 } else { *expected },
                        "depth",
                        pixel,
                        channel,
                    );
                }
                let x = pixel as u32 % WIDTH;
                let y = pixel as u32 / WIDTH;
                let [left, top, width, height] = RASTER_SCISSOR;
                if x < left || x >= left + width || y < top || y >= top + height {
                    assert_eq!(actual, [1.0; 4], "full depth clear outside Feature scissor");
                }
            }
        }
        assert_eq!(
            self.resolved_depth[0], self.resolved_depth[2],
            "preserved depth resolves identically before and after importing its original lease",
        );
        assert_ne!(
            self.resolved_depth[1], self.resolved_depth[2],
            "the second raster pass changes preserved depth without rewriting original depth",
        );
        let [left, top, width, height] = PRESERVE_SCISSOR;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                if x >= left && x < left + width && y >= top && y < top + height {
                    continue;
                }
                let start = ((y * WIDTH + x) * 16) as usize;
                assert_eq!(
                    self.color[0][start..start + 16],
                    self.color[1][start..start + 16]
                );
                assert_eq!(
                    self.resolved_depth[1][start..start + 16],
                    self.resolved_depth[2][start..start + 16],
                    "Preserve retains every original depth byte outside the second scissor",
                );
            }
        }
        let untouched = (13 * WIDTH as usize + 21) * 16;
        let far = f32::from_le_bytes(
            self.resolved_depth[2][untouched..untouched + 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            far, 0.9375,
            "Preserve retains far geometry outside the smaller second scissor"
        );
    }
}
