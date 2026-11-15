use nara_render_wgpu::{
    AccessRole, GraphLimits, WgpuFeatureAllocation, WgpuFeatureDefinition,
    WgpuFeatureEncodeContext, WgpuFeaturePrepareContext, WgpuProgramTexture,
    WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderHost, WgpuRenderProgramBuilder,
    WgpuRenderProgramError, WgpuSubmissionPrefix,
};
use std::{
    marker::PhantomData,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

struct CopyImage<S>(PhantomData<fn() -> S>);

struct Prepared<S: 'static> {
    source: WgpuProgramTexture<S>,
    report: Report,
}

#[derive(Clone)]
struct Report {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
    width: u32,
    height: u32,
    row: u32,
    format: wgpu::TextureFormat,
}

impl<S: 'static> WgpuRenderFeature for CopyImage<S> {
    type Capture = WgpuProgramTexture<S>;
    type State = ();
    type Prepared = Prepared<S>;
    type Report = Report;

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
    ) -> Result<Prepared<S>, WgpuRenderProgramError> {
        let native = &source.descriptor().native;
        if !matches!(
            native.format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) || native.size.width > 1024
            || native.size.height > 1024
            || native.size.depth_or_array_layers != 1
            || native.sample_count != 1
        {
            return Err(WgpuRenderProgramError::feature(
                "finite single-sample RGBA8 readback required",
            ));
        }
        let row = (native.size.width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bytes = u64::from(row) * u64::from(native.size.height);
        let allocation = context.reserve_bytes(bytes)?;
        let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("geometry-consumer-readback"),
            size: bytes,
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
        prepared: &Prepared<S>,
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

pub fn settle(prefix: &WgpuSubmissionPrefix) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !prefix.poll().expect("original submission completion") {
        assert!(
            Instant::now() < deadline,
            "original prefix completion deadline"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(prefix.cleanup_fault_count(), 0);
}

pub fn read<S: 'static>(
    host: &mut WgpuRenderHost,
    output: &WgpuProgramTextureOutput<S>,
) -> Vec<[u8; 4]> {
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(CopyImage::<S>(PhantomData)),
        )
        .unwrap();
    let mut graph = WgpuRenderProgramBuilder::new(
        output.source_generation(),
        output.frame_index(),
        GraphLimits::default(),
    )
    .unwrap();
    let imported = graph.import_texture(output).unwrap();
    graph
        .operation(
            1,
            &instance,
            Arc::new(imported.clone()),
            &[imported.read(AccessRole::CopySource)],
            &[],
        )
        .unwrap();
    let group = host
        .submit_program(&producer, &graph.finish(&[imported.output()]).unwrap())
        .unwrap();
    let report = group.report::<CopyImage<S>>(1, &instance).unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    report
        .buffer
        .map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut mapped = false;
    loop {
        let complete = group.submitted_prefix().poll().unwrap();
        report.device.poll(wgpu::PollType::Poll).unwrap();
        if !mapped {
            match receiver.try_recv() {
                Ok(result) => {
                    result.unwrap();
                    mapped = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("original map callback disconnected")
                }
            }
        }
        if mapped && complete {
            break;
        }
        assert!(Instant::now() < deadline, "readback deadline");
        thread::sleep(Duration::from_millis(1));
    }
    let mut pixels = Vec::with_capacity((report.width * report.height) as usize);
    {
        let data = report.buffer.get_mapped_range(..).unwrap();
        for row in data.chunks_exact(report.row as usize) {
            for pixel in row[..report.width as usize * 4].chunks_exact(4) {
                let mut pixel: [u8; 4] = pixel.try_into().unwrap();
                if matches!(
                    report.format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                ) {
                    pixel.swap(0, 2);
                }
                pixels.push(pixel);
            }
        }
    }
    report.buffer.unmap();
    settle(&host.retire_producer(&producer).unwrap());
    pixels
}

pub fn finish(host: &mut WgpuRenderHost) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !host.poll_retirement().unwrap() {
        assert!(
            Instant::now() < deadline,
            "original Host retirement deadline"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
    assert_eq!(host.cleanup_fault_count(), 0);
}
