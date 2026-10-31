//! A declared public readback Feature; the Host owns exact imports and submission custody.

use std::{
    marker::PhantomData,
    sync::Arc,
    sync::mpsc::{self, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use nara_render_wgpu::{
    AccessRole, WgpuFeatureAllocation, WgpuFeatureEncodeContext, WgpuFeatureInstance,
    WgpuFeaturePreflightContext, WgpuFeaturePrepareContext, WgpuProgramOutputs, WgpuProgramTexture,
    WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderHost, WgpuRenderProgramBuilder,
    WgpuRenderProgramError, WgpuSubmissionPrefix,
};

use super::{
    drawing_gpu::{WrittenTexture, install},
    drawing_model::{FINAL_BYTES, HEIGHT, SCENE_HEIGHT, SCENE_WIDTH, WIDTH, limits, source},
};

pub const TIMEOUT: Duration = Duration::from_secs(10);
const READBACK: u64 = 1;

pub struct Rendered<S: 'static> {
    pub output: WgpuProgramTextureOutput<S>,
}

impl<S: 'static> Rendered<S> {
    pub fn published<F: WgpuRenderFeature<Report = WrittenTexture<S>>>(
        group: &WgpuProgramOutputs,
        key: u64,
        instance: &WgpuFeatureInstance<F>,
    ) -> Self {
        let report = group
            .report(key, instance)
            .expect("this original Feature's typed report");
        Self {
            output: group
                .texture(&report.port)
                .expect("publish the report's exact original port"),
        }
    }

    pub fn read(&self, host: &mut WgpuRenderHost) -> Vec<[f32; 4]> {
        assert!(
            self.output.is_current(),
            "readback must retain its original live epoch"
        );
        let producer = host
            .create_producer()
            .expect("admit an independent numeric readback producer");
        let instance = install(host, &producer, Readback::<S>(PhantomData));
        let mut builder = WgpuRenderProgramBuilder::new(source(), 1, limits()).unwrap();
        let imported = builder
            .import_texture(&self.output)
            .expect("import the exact retained output");
        builder
            .operation(
                READBACK,
                &instance,
                Arc::new(imported.clone()),
                &[imported.read(AccessRole::CopySource)],
                &[],
            )
            .expect("declare the readback Feature's exact copy-source dependency");
        // Re-export the exact imported storage to keep the complete output group nonempty.
        // The readback operation declares no texture writes and creates no successor pixels.
        let program = builder
            .finish(&[imported.output()])
            .expect("finish the readback report with its exact pass-through output");
        let group = host
            .submit_program(&producer, &program)
            .expect("the Host must submit the declared exact-output readback Feature");
        {
            let pass_through = group
                .texture(&imported)
                .expect("return the exact imported typed output");
            assert_eq!(
                pass_through.producer_id(),
                self.output.producer_id(),
                "pass-through output keeps its original producer"
            );
            assert_eq!(
                pass_through.source_generation(),
                self.output.source_generation(),
                "pass-through output keeps its original source"
            );
            assert_eq!(
                pass_through.frame_index(),
                self.output.frame_index(),
                "pass-through output keeps its original frame"
            );
            assert_eq!(pass_through.descriptor(), self.output.descriptor());
            assert!(pass_through.is_current());
        }
        let pixels = group
            .report::<Readback<S>>(READBACK, &instance)
            .expect("the original readback Feature's successful typed report")
            .read(group.submitted_prefix());
        let prefix = host
            .retire_producer(&producer)
            .expect("the independent readback producer owns its actual submitted prefix");
        assert!(
            prefix.is_complete(),
            "mapping and original prefix complete before readback retirement"
        );
        drop(group);
        pixels
    }
}

pub struct Readback<S: 'static>(PhantomData<fn() -> S>);

pub struct ReadbackPrepared<S: 'static> {
    source: WgpuProgramTexture<S>,
    report: ReadbackReport,
}

#[derive(Clone)]
pub struct ReadbackReport {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
    extent: wgpu::Extent3d,
    padded_row: u32,
}

impl<S: 'static> WgpuRenderFeature for Readback<S> {
    type Capture = WgpuProgramTexture<S>;
    type State = ();
    type Prepared = ReadbackPrepared<S>;
    type Report = ReadbackReport;

    fn preflight(
        &self,
        port: &Self::Capture,
        _: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let descriptor = port.descriptor();
        let extent = descriptor.native.size;
        if !matches!(
            (extent.width, extent.height),
            (SCENE_WIDTH, SCENE_HEIGHT) | (WIDTH, HEIGHT)
        ) || extent.depth_or_array_layers != 1
            || descriptor.native.format != wgpu::TextureFormat::Rgba32Float
            || !descriptor
                .native
                .usage
                .contains(wgpu::TextureUsages::COPY_SRC)
        {
            return Err(WgpuRenderProgramError::feature(
                "readback requires a finite 8x6 or 32x24 RGBA32Float source",
            ));
        }
        Ok(())
    }

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn prepare(
        &self,
        _: &mut (),
        port: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let extent = port.descriptor().native.size;
        let row_bytes = extent.width * 16;
        let padded_row = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let size = u64::from(padded_row) * u64::from(extent.height);
        assert!(
            size > 0 && size <= FINAL_BYTES,
            "finite private readback allocation"
        );
        let allocation = context.reserve_bytes(size)?;
        let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("finite-drawing-numeric-readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(ReadbackPrepared {
            source: port.clone(),
            report: ReadbackReport {
                device: context.device().clone(),
                buffer,
                _allocation: allocation,
                extent,
                padded_row,
            },
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<ReadbackReport, WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        encoder.copy_texture_to_buffer(
            resources.texture_native(&prepared.source)?.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &prepared.report.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(prepared.report.padded_row),
                    rows_per_image: Some(prepared.report.extent.height),
                },
            },
            prepared.report.extent,
        );
        Ok(prepared.report.clone())
    }
}

impl ReadbackReport {
    fn read(&self, prefix: &WgpuSubmissionPrefix) -> Vec<[f32; 4]> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
        let deadline = Instant::now() + TIMEOUT;
        let mut mapped = false;
        loop {
            let complete = prefix
                .poll()
                .expect("service the Host's original readback prefix");
            self.device
                .poll(wgpu::PollType::Poll)
                .expect("poll the original device's map callback");
            if !mapped {
                match receiver.try_recv() {
                    Ok(result) => {
                        result.expect("the native readback must map successfully");
                        mapped = true;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        panic!("native readback callback disappeared")
                    }
                }
            }
            if mapped && complete {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native readback exceeded ten seconds"
            );
            thread::sleep(Duration::from_millis(1));
        }
        let extent = self.extent;
        let row_bytes = extent.width * 16;
        let pixels = {
            let mapped = self
                .buffer
                .get_mapped_range(..)
                .expect("readback buffer is mapped");
            assert_eq!(
                mapped.len() as u64,
                u64::from(self.padded_row) * u64::from(extent.height)
            );
            let mut pixels = Vec::with_capacity((extent.width * extent.height) as usize);
            for row in mapped.chunks_exact(self.padded_row as usize) {
                for pixel in row[..row_bytes as usize].chunks_exact(16) {
                    pixels.push(std::array::from_fn(|channel| {
                        f32::from_le_bytes(pixel[channel * 4..channel * 4 + 4].try_into().unwrap())
                    }));
                }
            }
            pixels
        };
        self.buffer.unmap();
        assert_eq!(pixels.len(), (extent.width * extent.height) as usize);
        pixels
    }
}

pub fn settle(prefix: &WgpuSubmissionPrefix) {
    let deadline = Instant::now() + TIMEOUT;
    while !prefix
        .poll()
        .expect("the original native prefix remains pollable")
    {
        assert!(
            Instant::now() < deadline,
            "native prefix exceeded ten seconds"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

/// Public maintenance polls the failed attempt's prefix without stopping any live producer.
pub fn await_retained(host: &mut WgpuRenderHost, host_frame: u64, expected: u64) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        host.begin_frame(host_frame)
            .expect("maintain the same live offscreen Host");
        if host.retained_resource_bytes() == expected {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "failed-attempt custody did not retire: retained={}, expected={expected}",
            host.retained_resource_bytes()
        );
        thread::sleep(Duration::from_millis(1));
    }
}

pub fn finish(host: &mut WgpuRenderHost) {
    let deadline = Instant::now() + TIMEOUT;
    while !host
        .poll_retirement()
        .expect("retire the owning Host's original native queue")
    {
        assert!(
            Instant::now() < deadline,
            "native Host retirement exceeded ten seconds"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
