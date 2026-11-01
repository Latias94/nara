//! Declared typed copy Feature. The Host retains allocation and original submission custody.

use std::{
    marker::PhantomData,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use nara_render_wgpu::{
    AccessRole, GraphLimits, WgpuFeatureAllocation, WgpuFeatureDefinition,
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuProgramTexture, WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderHost,
    WgpuRenderProgramBuilder, WgpuRenderProgramError, WgpuSubmissionPrefix,
};

use super::oracle::Image;

pub struct Readback<S: 'static>(PhantomData<fn() -> S>);

pub struct Prepared<S: 'static> {
    source: WgpuProgramTexture<S>,
    report: Report,
}

#[derive(Clone)]
pub struct Report {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
    width: u32,
    height: u32,
    row: u32,
    format: wgpu::TextureFormat,
}

impl<S: 'static> WgpuRenderFeature for Readback<S> {
    type Capture = WgpuProgramTexture<S>;
    type State = ();
    type Prepared = Prepared<S>;
    type Report = Report;

    fn preflight(
        &self,
        source: &Self::Capture,
        _: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let native = &source.descriptor().native;
        if !matches!(
            native.format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) || native.size.width == 0
            || native.size.width > 2048
            || native.size.height == 0
            || native.size.height > 2048
            || native.size.depth_or_array_layers != 1
            || native.sample_count != 1
            || !native.usage.contains(wgpu::TextureUsages::COPY_SRC)
        {
            return Err(WgpuRenderProgramError::feature(
                "finite RGBA8/BGRA8 copy-source required",
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
        source: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let native = &source.descriptor().native;
        let row = (native.size.width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let size = u64::from(row) * u64::from(native.size.height);
        let allocation = context.reserve_bytes(size)?;
        let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("stock-consumer-declared-readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Prepared {
            source: source.clone(),
            report: Report {
                device: context.device().clone(),
                buffer,
                _allocation: allocation,
                width: native.size.width,
                height: native.size.height,
                row,
                format: native.format,
            },
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Report, WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        encoder.copy_texture_to_buffer(
            resources.texture_native(&prepared.source)?.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &prepared.report.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(prepared.report.row),
                    rows_per_image: Some(prepared.report.height),
                },
            },
            wgpu::Extent3d {
                width: prepared.report.width,
                height: prepared.report.height,
                depth_or_array_layers: 1,
            },
        );
        Ok(prepared.report.clone())
    }
}

pub fn read<S: 'static>(host: &mut WgpuRenderHost, output: &WgpuProgramTextureOutput<S>) -> Image {
    assert!(output.is_current());
    let producer = host
        .create_producer()
        .expect("independent declared-readback producer");
    let definition = WgpuFeatureDefinition::new(Readback::<S>(PhantomData));
    let instance = host
        .create_feature_instance(&producer, &definition)
        .unwrap();
    let mut builder = WgpuRenderProgramBuilder::new(
        output.source_generation(),
        output.frame_index(),
        GraphLimits::default(),
    )
    .unwrap();
    let imported = builder.import_texture(output).unwrap();
    builder
        .operation(
            1,
            &instance,
            Arc::new(imported.clone()),
            &[imported.read(AccessRole::CopySource)],
            &[],
        )
        .unwrap();
    let group = host
        .submit_program(&producer, &builder.finish(&[imported.output()]).unwrap())
        .unwrap();
    let retained = group.texture(&imported).unwrap();
    assert_eq!(retained.producer_id(), output.producer_id());
    assert_eq!(retained.source_generation(), output.source_generation());
    assert_eq!(retained.frame_index(), output.frame_index());
    let report = group.report::<Readback<S>>(1, &instance).unwrap();
    let image = report.read(group.submitted_prefix());
    assert!(host.retire_producer(&producer).unwrap().is_complete());
    image
}

impl Report {
    fn read(&self, prefix: &WgpuSubmissionPrefix) -> Image {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut mapped = false;
        loop {
            let complete = prefix
                .poll()
                .expect("poll the actual Host submission prefix");
            self.device
                .poll(wgpu::PollType::Poll)
                .expect("poll the original map callback");
            if !mapped {
                match receiver.try_recv() {
                    Ok(result) => {
                        result.expect("map the actual readback buffer");
                        mapped = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        panic!("readback callback disconnected")
                    }
                }
            }
            if mapped && complete {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "declared readback exceeded ten seconds"
            );
            thread::sleep(Duration::from_millis(1));
        }
        let mut pixels = Vec::with_capacity((self.width * self.height) as usize);
        {
            let mapped = self.buffer.get_mapped_range(..).unwrap();
            for row in mapped.chunks_exact(self.row as usize) {
                for bytes in row[..self.width as usize * 4].chunks_exact(4) {
                    let mut rgba: [u8; 4] = bytes.try_into().unwrap();
                    if matches!(
                        self.format,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                    ) {
                        rgba.swap(0, 2);
                    }
                    pixels.push(rgba);
                }
            }
        }
        self.buffer.unmap();
        Image {
            width: self.width,
            height: self.height,
            format: self.format,
            pixels,
        }
    }
}

pub fn finish(host: &mut WgpuRenderHost) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !host
        .poll_retirement()
        .expect("retire the original offscreen Host")
    {
        assert!(
            Instant::now() < deadline,
            "Host retirement exceeded ten seconds"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

pub fn require_native_selection() {
    let backend = std::env::var("WGPU_BACKEND").expect("explicit WGPU_BACKEND required");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
}
