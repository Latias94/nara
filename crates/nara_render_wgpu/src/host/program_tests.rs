use super::*;
use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDefinition,
    WgpuFeatureEncodeContext, WgpuFeaturePrepareContext, WgpuProgramTexture,
    WgpuProgramTextureDescriptor, WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderProgram,
    WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

struct Color;

fn source() -> RenderFrameSourceGeneration {
    RenderFrameSourceGeneration::try_acquire().unwrap()
}

fn descriptor() -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("host-program-test-color"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    }
}

fn clear(
    source: RenderFrameSourceGeneration,
    frame: u64,
    color: wgpu::Color,
) -> (WgpuRenderProgram, WgpuProgramTexture<Color>) {
    let mut builder = WgpuRenderProgramBuilder::new(source, frame, GraphLimits::default()).unwrap();
    let output = builder.texture::<Color>(descriptor()).unwrap();
    builder
        .initialize(3, &[output.clear(color, AccessRole::Attachment)])
        .unwrap();
    (builder.finish(&[output.output()]).unwrap(), output)
}

fn drain(prefix: &WgpuSubmissionPrefix) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !prefix.poll().unwrap() {
        assert!(
            std::time::Instant::now() < deadline,
            "the original prefix did not settle"
        );
        std::thread::yield_now();
    }
}

fn pixels(host: &mut WgpuRenderHost, output: &WgpuProgramTextureOutput<Color>) -> Vec<[u8; 4]> {
    let namespace = host.prepare_namespace(None).unwrap();
    let crate::program::NativeResource::Texture { texture, .. } = &output.storage.native else {
        unreachable!()
    };
    let readback = namespace.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("host-program-readback"),
        size: 4 * 256,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = namespace
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(4),
            },
        },
        texture.size(),
    );
    let retention = WgpuSubmissionReservation::new(
        &namespace.device,
        &namespace.queue,
        (output.clone(), readback.clone()),
    );
    let prefix = retention.submitted(namespace.queue.submit([encoder.finish()]));
    let (send, receive) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        send.send(result).unwrap()
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        namespace.device.poll(wgpu::PollType::Poll).unwrap();
        match receive.try_recv() {
            Ok(result) => {
                result.unwrap();
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(std::time::Instant::now() < deadline)
            }
            Err(error) => panic!("readback callback disconnected: {error}"),
        }
        std::thread::yield_now();
    }
    let result = {
        let mapped = readback.get_mapped_range(..).unwrap();
        mapped
            .chunks_exact(256)
            .flat_map(|row| {
                row[..16]
                    .chunks_exact(4)
                    .map(|pixel| pixel.try_into().unwrap())
            })
            .collect()
    };
    readback.unmap();
    drain(&prefix);
    result
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_program_submission_failure_keeps_the_entire_group_and_capture_unpublished() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let source = source();
    host.begin_frame(1).unwrap();
    let (first, first_port) = clear(source, 1, wgpu::Color::RED);
    let first = host
        .submit_program(&producer, &first)
        .unwrap()
        .texture(&first_port)
        .unwrap();
    let mut builder = WgpuRenderProgramBuilder::new(source, 2, GraphLimits::default()).unwrap();
    let second_port = builder.texture::<Color>(descriptor()).unwrap();
    let counter = builder
        .buffer::<u32>(wgpu::BufferDescriptor {
            label: Some("atomic-group-counter"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
        .unwrap();
    builder
        .initialize(
            1,
            &[
                second_port.clear(wgpu::Color::GREEN, AccessRole::Attachment),
                counter.zero(AccessRole::StorageWrite),
            ],
        )
        .unwrap();
    let second = builder
        .finish(&[counter.output(), second_port.output()])
        .unwrap();
    PROGRAM_SUBMISSION_FAILURE.with(|failure| failure.set(true));
    assert!(matches!(
        host.submit_program(&producer, &second),
        Err(WgpuRenderHostError::Backend(
            WgpuRenderError::ContentRejected { .. }
        ))
    ));
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    assert!(
        pixels(&mut host, &first)
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    let second_group = host.submit_program(&producer, &second).unwrap();
    assert_eq!(second_group.buffer(&counter).unwrap().frame_index(), 2);
    let second = second_group.texture(&second_port).unwrap();
    assert!(
        pixels(&mut host, &second)
            .iter()
            .all(|pixel| *pixel == [0, 255, 0, 255])
    );
    assert!(
        pixels(&mut host, &first)
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(2)
    );
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_program_outputs_do_not_retain_scratch_or_reconstruction_inputs_after_settlement() {
    const COLOR_BYTES: u64 = 4 * 4 * 4;
    let mut host = WgpuRenderHost::offscreen().with_output_byte_limit(2 * COLOR_BYTES - 1);
    let producer = host.create_producer().unwrap();
    host.begin_frame(1).unwrap();
    let mut builder = WgpuRenderProgramBuilder::new(source(), 1, GraphLimits::default()).unwrap();
    let scratch = builder.texture::<Color>(descriptor()).unwrap();
    let output = builder.texture::<Color>(descriptor()).unwrap();
    builder
        .initialize(
            9,
            &[scratch.clear(wgpu::Color::BLUE, AccessRole::Attachment)],
        )
        .unwrap();
    builder
        .initialize(1, &[output.preserve(&scratch, AccessRole::Attachment)])
        .unwrap();
    let program = builder.finish(&[output.output()]).unwrap();
    let weak_program = Arc::downgrade(&program.0);
    assert!(matches!(
        host.submit_program(&producer, &program),
        Err(WgpuRenderHostError::Program(
            WgpuRenderProgramError::ResourceCapacity
        ))
    ));
    drain(
        host.views[&producer.id()]
            .submitted_prefix
            .as_ref()
            .unwrap(),
    );
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);
    assert_eq!(host.retained_output_bytes(), 0);
    host = host.with_output_byte_limit(2 * COLOR_BYTES);
    let outputs = host.submit_program(&producer, &program).unwrap();
    let lease = outputs.texture(&output).unwrap();
    assert!(matches!(
        outputs.texture(&scratch),
        Err(WgpuRenderProgramError::UnpublishedResource)
    ));
    let prefix = outputs.submitted_prefix().clone();
    drop(program);
    drain(&prefix);
    assert!(weak_program.upgrade().is_none());
    assert_eq!(host.retained_output_bytes(), COLOR_BYTES);
    assert!(
        pixels(&mut host, &lease)
            .iter()
            .all(|pixel| *pixel == [0, 0, 255, 255])
    );
    drop(outputs);
    assert_eq!(host.retained_output_bytes(), COLOR_BYTES);
    drop(lease);
    assert_eq!(host.retained_output_bytes(), 0);
}

struct RejectOnce;
struct RejectCapture(Arc<AtomicBool>);

impl WgpuRenderFeature for RejectOnce {
    type Capture = RejectCapture;
    type State = ();
    type Prepared = ();
    type Report = ();
    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }
    fn prepare(
        &self,
        _: &mut (),
        capture: &RejectCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        if capture.0.swap(false, Ordering::AcqRel) {
            let allocation = context.reserve_bytes(16)?;
            let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("rejected-feature-upload"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            context.upload_buffer(&buffer, 0, &[42; 16])?;
            drop(allocation);
            return Err(WgpuRenderProgramError::feature(
                "injected rejection after private upload",
            ));
        }
        Ok(())
    }
    fn encode(
        &self,
        _: &(),
        _: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_rejected_feature_flushes_original_uploads_and_can_retry_the_same_capture() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(&producer, &WgpuFeatureDefinition::new(RejectOnce))
        .unwrap();
    host.begin_frame(1).unwrap();
    let mut builder = WgpuRenderProgramBuilder::new(source(), 1, GraphLimits::default()).unwrap();
    let output = builder.texture::<Color>(descriptor()).unwrap();
    builder
        .operation(
            1,
            &instance,
            Arc::new(RejectCapture(Arc::new(AtomicBool::new(true)))),
            &[],
            &[output.clear(wgpu::Color::BLUE, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder.finish(&[output.output()]).unwrap();
    assert!(matches!(
        host.submit_program(&producer, &program),
        Err(WgpuRenderHostError::Program(
            WgpuRenderProgramError::Feature { .. }
        ))
    ));
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);
    drain(
        host.views[&producer.id()]
            .submitted_prefix
            .as_ref()
            .unwrap(),
    );
    assert_eq!(host.retained_output_bytes(), 0);
    let outputs = host.submit_program(&producer, &program).unwrap();
    assert_eq!(outputs.report::<RejectOnce>(1, &instance).unwrap(), &());
    let output = outputs.texture(&output).unwrap();
    assert!(
        pixels(&mut host, &output)
            .iter()
            .all(|pixel| *pixel == [0, 0, 255, 255])
    );
}
