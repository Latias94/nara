//! Ordinary typed program imports, guarded staging and exact final-image use under the Host.

use std::{
    sync::{
        Arc,
        mpsc::{self, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    AccessRole, WgpuFeatureAllocation, WgpuFeatureDefinition, WgpuFeatureEncodeContext,
    WgpuFeatureInstance, WgpuFeaturePrepareContext, WgpuImageOutput, WgpuProgramTexture,
    WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderHost, WgpuRenderProducer,
    WgpuRenderProgramBuilder, WgpuRenderProgramError, WgpuSubmissionPrefix,
};

use super::accumulation::{HEIGHT, HistoryColor, VIEW_WIDTH, WIDTH, limits};

pub const TIMEOUT: Duration = Duration::from_secs(10);
const HISTORY_ROW: u32 = VIEW_WIDTH * 16;
const HISTORY_BYTES: u64 = HISTORY_ROW as u64 * HEIGHT as u64;
const FINAL_ROW: u32 =
    (WIDTH * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
const FINAL_OFFSET: u64 = 4 * HISTORY_BYTES;
const STAGING_BYTES: u64 = FINAL_OFFSET + FINAL_ROW as u64 * HEIGHT as u64;

pub struct Readback;

pub struct Prepared {
    ports: [WgpuProgramTexture<HistoryColor>; 4],
    report: Report,
}

#[derive(Clone)]
pub struct Report {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

pub struct Samples {
    pub histories: [Vec<[f32; 4]>; 4],
    pub image: Vec<[u8; 4]>,
}

impl WgpuRenderFeature for Readback {
    type Capture = [WgpuProgramTexture<HistoryColor>; 4];
    type State = ();
    type Prepared = Prepared;
    type Report = Report;

    fn create_state(
        &self,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn prepare(
        &self,
        _state: &mut (),
        ports: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Prepared, WgpuRenderProgramError> {
        let allocation = context.reserve_bytes(STAGING_BYTES)?;
        let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("independent-history-guarded-readback"),
            size: STAGING_BYTES,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Prepared {
            ports: ports.clone(),
            report: Report {
                device: context.device().clone(),
                buffer,
                _allocation: allocation,
            },
        })
    }

    fn encode(
        &self,
        prepared: &Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Report, WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        for (index, port) in prepared.ports.iter().enumerate() {
            encoder.copy_texture_to_buffer(
                resources.texture_native(port)?.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &prepared.report.buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: index as u64 * HISTORY_BYTES,
                        bytes_per_row: Some(HISTORY_ROW),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                wgpu::Extent3d {
                    width: VIEW_WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(prepared.report.clone())
    }
}

pub struct Reader {
    producer: WgpuRenderProducer,
    instance: WgpuFeatureInstance<Readback>,
    source: RenderFrameSourceGeneration,
    frame: u64,
}

impl Reader {
    pub fn new(host: &mut WgpuRenderHost) -> Self {
        let producer = host.create_producer().unwrap();
        let definition = WgpuFeatureDefinition::new(Readback);
        let instance = host
            .create_feature_instance(&producer, &definition)
            .unwrap();
        Self {
            producer,
            instance,
            source: RenderFrameSourceGeneration::try_acquire().unwrap(),
            frame: 0,
        }
    }

    pub fn read(
        &mut self,
        host: &mut WgpuRenderHost,
        histories: &[WgpuProgramTextureOutput<HistoryColor>; 4],
        image: &WgpuImageOutput,
    ) -> Samples {
        assert_eq!(HISTORY_ROW % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
        assert_eq!(image.format(), wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(
            (image.extent().width, image.extent().height),
            (WIDTH, HEIGHT)
        );
        self.frame += 1;
        let mut builder = WgpuRenderProgramBuilder::new(self.source, self.frame, limits()).unwrap();
        let ports = [
            builder.import_texture(&histories[0]).unwrap(),
            builder.import_texture(&histories[1]).unwrap(),
            builder.import_texture(&histories[2]).unwrap(),
            builder.import_texture(&histories[3]).unwrap(),
        ];
        let reads = ports
            .each_ref()
            .map(|port| port.read(AccessRole::CopySource));
        builder
            .operation(1, &self.instance, Arc::new(ports.clone()), &reads, &[])
            .unwrap();
        let program = builder
            .finish(&ports.each_ref().map(|port| port.output()))
            .unwrap();
        let outputs = host
            .submit_program(&self.producer, &program)
            .expect("the ordinary Host must publish this complete imported readback group");
        let report = outputs
            .report::<Readback>(1, &self.instance)
            .expect("only the exact original readback instance can retrieve its report");

        // The image seam supplies its exact texture. Staging was already allocated and charged
        // by the ordinary Feature above; retain the report until both original prefixes settle.
        let image_copy = host.submit_image_use(
            std::slice::from_ref(image),
            |_device, _queue, encoder, views| {
                encoder.copy_texture_to_buffer(
                    views[0].texture().as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &report.buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: FINAL_OFFSET,
                            bytes_per_row: Some(FINAL_ROW),
                            rows_per_image: Some(HEIGHT),
                        },
                    },
                    wgpu::Extent3d {
                        width: WIDTH,
                        height: HEIGHT,
                        depth_or_array_layers: 1,
                    },
                );
                Ok(())
            },
        );
        let (_, image_prefix) = match image_copy {
            Ok(submitted) => submitted,
            Err(error) => {
                // Keep the staging guard pinned while the failed native attempt retires.
                finish(host);
                panic!("exact final-image readback failed: {error}");
            }
        };
        report.read(outputs.submitted_prefix(), &image_prefix)
    }

    pub fn retire(self, host: &mut WgpuRenderHost) {
        if let Some(prefix) = host.retire_producer(&self.producer) {
            settle(&prefix);
        }
    }
}

impl Report {
    fn read(
        &self,
        history_prefix: &WgpuSubmissionPrefix,
        image_prefix: &WgpuSubmissionPrefix,
    ) -> Samples {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
        let deadline = Instant::now() + TIMEOUT;
        loop {
            history_prefix
                .poll()
                .expect("poll the original history-copy prefix");
            image_prefix
                .poll()
                .expect("poll the original final-image-copy prefix");
            self.device
                .poll(wgpu::PollType::Poll)
                .expect("service the original guarded staging buffer's map callback");
            match receiver.try_recv() {
                Ok(result) => {
                    result.expect("native readback mapping must succeed");
                    break;
                }
                Err(TryRecvError::Empty) => {
                    assert!(
                        Instant::now() < deadline,
                        "history readback mapping timed out"
                    );
                    thread::sleep(Duration::from_millis(1));
                }
                Err(TryRecvError::Disconnected) => panic!("the history map callback disappeared"),
            }
        }
        settle(history_prefix);
        settle(image_prefix);
        let samples = {
            let mapped = self.buffer.get_mapped_range(..).expect("staging is mapped");
            assert_eq!(mapped.len() as u64, STAGING_BYTES);
            let histories = std::array::from_fn(|index| {
                let start = index * HISTORY_BYTES as usize;
                mapped[start..start + HISTORY_BYTES as usize]
                    .chunks_exact(16)
                    .map(|pixel| {
                        std::array::from_fn(|channel| {
                            f32::from_le_bytes(
                                pixel[channel * 4..channel * 4 + 4].try_into().unwrap(),
                            )
                        })
                    })
                    .collect()
            });
            let image = mapped[FINAL_OFFSET as usize..]
                .chunks_exact(FINAL_ROW as usize)
                .flat_map(|row| row[..(WIDTH * 4) as usize].chunks_exact(4))
                .map(|pixel| pixel.try_into().unwrap())
                .collect();
            Samples { histories, image }
        };
        self.buffer.unmap();
        samples
    }
}

pub fn require_native_selection() {
    let backend = std::env::var("WGPU_BACKEND").expect("explicitly select WGPU_BACKEND");
    assert!(
        matches!(backend.as_str(), "dx12" | "vulkan" | "metal"),
        "select exactly one native backend: dx12, vulkan or metal"
    );
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0"),
        "native qualification requires NARA_WGPU_FORCE_FALLBACK=0"
    );
}

pub fn settle(prefix: &WgpuSubmissionPrefix) {
    let deadline = Instant::now() + TIMEOUT;
    while !prefix.poll().expect("poll the original submitted prefix") {
        assert!(
            Instant::now() < deadline,
            "original submission prefix timed out"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(prefix.is_complete());
}

pub fn finish(host: &mut WgpuRenderHost) {
    let deadline = Instant::now() + TIMEOUT;
    while !host
        .poll_retirement()
        .expect("poll the original Host's retirement")
    {
        assert!(Instant::now() < deadline, "Host retirement timed out");
        thread::sleep(Duration::from_millis(1));
    }
}
