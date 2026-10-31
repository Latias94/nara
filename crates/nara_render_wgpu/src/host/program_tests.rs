use super::*;
use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDefinition,
    WgpuFeatureEncodeContext, WgpuFeaturePrepareContext, WgpuProgramTexture,
    WgpuProgramTextureDescriptor, WgpuProgramTextureOutput, WgpuRenderFeature, WgpuRenderProgram,
    WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

struct PreflightProbe {
    events: Arc<std::sync::Mutex<Vec<&'static str>>>,
    created: Arc<std::sync::atomic::AtomicUsize>,
}

struct PreflightCapture {
    name: &'static str,
    reject: bool,
}

impl WgpuRenderFeature for PreflightProbe {
    type Capture = PreflightCapture;
    type State = ();
    type Prepared = ();
    type Report = ();
    fn preflight(
        &self,
        capture: &PreflightCapture,
        context: &mut crate::WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        assert!(context.limits().max_texture_dimension_2d >= 4);
        self.events.lock().unwrap().push(capture.name);
        if capture.reject {
            return Err(WgpuRenderError::UploadDeferred {
                message: "injected later-capture preflight refusal".into(),
            }
            .into());
        }
        Ok(())
    }
    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        self.events.lock().unwrap().push("state");
        self.created.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    fn prepare(
        &self,
        _: &mut (),
        _: &PreflightCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        self.events.lock().unwrap().push("prepare");
        let allocation = context.reserve_bytes(16)?;
        let upload = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("preflight-probe-private-upload"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        context.upload_buffer(&upload, 0, &[23; 16])?;
        drop(allocation);
        Ok(())
    }
    fn encode(
        &self,
        _: &(),
        _: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        self.events.lock().unwrap().push("encode");
        Ok(())
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_complete_preflight_precedes_allocations_uploads_and_preserves_untouched_state() {
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let created = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(0);
    let producer = host.create_producer().unwrap();
    let definition = WgpuFeatureDefinition::new(PreflightProbe {
        events: Arc::clone(&events),
        created: Arc::clone(&created),
    });
    let a = host
        .create_feature_instance(&producer, &definition)
        .unwrap();
    let b = host
        .create_feature_instance(&producer, &definition)
        .unwrap();
    let source = source();
    let build = |frame, reject| {
        let mut builder =
            WgpuRenderProgramBuilder::new(source, frame, GraphLimits::default()).unwrap();
        let first = builder.texture::<Color>(descriptor()).unwrap();
        let second = builder.texture::<Color>(descriptor()).unwrap();
        builder
            .operation(
                2,
                &b,
                Arc::new(PreflightCapture {
                    name: "check-b",
                    reject,
                }),
                &[],
                &[second.preserve(&first, AccessRole::Attachment)],
            )
            .unwrap();
        builder
            .operation(
                1,
                &a,
                Arc::new(PreflightCapture {
                    name: "check-a",
                    reject: false,
                }),
                &[],
                &[first.clear(wgpu::Color::RED, AccessRole::Attachment)],
            )
            .unwrap();
        builder.finish(&[second.output()]).unwrap()
    };
    host.begin_frame(1).unwrap();
    let error = host.submit_program(&producer, &build(1, true)).unwrap_err();
    assert!(matches!(
        error,
        WgpuRenderHostError::Backend(WgpuRenderError::UploadDeferred { .. })
    ));
    assert!(error.is_deferred());
    assert_eq!(*events.lock().unwrap(), ["check-a", "check-b"]);
    assert_eq!(created.load(Ordering::Acquire), 0);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);

    events.lock().unwrap().clear();
    host = host.with_resource_byte_limit(1024);
    let outputs = host.submit_program(&producer, &build(1, false)).unwrap();
    drain(outputs.submitted_prefix());
    assert_eq!(
        *events.lock().unwrap(),
        [
            "check-a", "check-b", "state", "prepare", "encode", "state", "prepare", "encode"
        ]
    );
    assert_eq!(created.load(Ordering::Acquire), 2);
    events.lock().unwrap().clear();
    let error = host.submit_program(&producer, &build(2, true)).unwrap_err();
    assert!(error.is_deferred());
    assert_eq!(*events.lock().unwrap(), ["check-a", "check-b"]);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    events.lock().unwrap().clear();
    let next = host.submit_program(&producer, &build(2, false)).unwrap();
    drain(next.submitted_prefix());
    assert_eq!(
        created.load(Ordering::Acquire),
        2,
        "a pure preflight refusal cannot evict untouched native state"
    );
    assert_eq!(
        *events.lock().unwrap(),
        [
            "check-a", "check-b", "prepare", "encode", "prepare", "encode"
        ]
    );
}

struct Color;

#[cfg(feature = "sprite-submitter")]
#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_stock_known_custody_refuses_before_partial_realization() {
    use crate::{
        quad::{WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture},
        sprite::{SpriteCapture, SpriteFeature},
    };
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(0);
    let producer = host.create_producer().unwrap();
    let instance = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(SpriteFeature::<Color>::new()),
        )
        .unwrap();
    let mut builder = WgpuRenderProgramBuilder::new(
        source(),
        1,
        GraphLimits {
            max_modeled_metadata_bytes: usize::MAX,
            ..Default::default()
        },
    )
    .unwrap();
    let target = builder.texture::<Color>(descriptor()).unwrap();
    let texture = WgpuQuadTexture::Fallback;
    let batch = WgpuQuadBatch {
        material: WgpuQuadMaterialKey {
            texture: texture.key(),
            sampler: nara_material::SamplerDescriptor::default(),
            alpha_mode: nara_material::AlphaMode2d::Blend,
        },
        texture,
        scissor: None,
        counts_as_sprites: true,
        material_extension: None,
        instances: vec![<WgpuQuadInstance as bytemuck::Zeroable>::zeroed()],
    };
    let capture = SpriteCapture {
        target: target.clone(),
        viewport: nara_render::ViewportRect::from_extent(nara_render::Extent2d::new(4, 4).unwrap())
            .unwrap(),
        batches: vec![batch; 20_000].into(),
    };
    builder
        .operation(
            1,
            &instance,
            Arc::new(capture),
            &[],
            &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder.finish(&[target.output()]).unwrap();
    host.begin_frame(1).unwrap();
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(matches!(
        error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::Graph(
            crate::GraphError::LimitExceeded(crate::LimitKind::SubmissionResources)
        ))
    ));
    assert!(!error.is_deferred());
    assert!(error.is_content_rejected());
    assert_eq!(host.retained_resource_bytes(), 0);
    assert!(!host.backend.texture_stats().has_fallback_texture);
    assert_eq!(
        host.backend.stock_quad_resources().instance_charges(),
        (0, 0)
    );
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);
}

#[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_stock_features_share_complete_image_preflight_and_original_resource_custody() {
    use crate::quad::{
        WGPU_QUAD_SHAPE_PLAIN, WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey,
        WgpuQuadTexture,
    };
    use crate::sprite::{SpriteCapture, SpriteFeature};
    use crate::ui::{UiCapture, UiFeature};
    use nara_material::{AlphaMode2d, SamplerDescriptor};
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(0);
    let producer = host.create_producer().unwrap();
    let sprites = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(SpriteFeature::<Color>::new()),
        )
        .unwrap();
    let ui = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(UiFeature::<Color>::new()),
        )
        .unwrap();
    let [red, green] = crate::readback_tests::two_image_versions();
    *host.backend.stock_quad_resources().textures_mut() =
        crate::texture::WgpuSpriteTextureCache::with_image_residency_limits(1, 1024);
    let mut descriptor = descriptor();
    descriptor.native.size.width = 64;
    descriptor.native.size.height = 64;
    let viewport =
        nara_render::ViewportRect::from_extent(nara_render::Extent2d::new(64, 64).unwrap())
            .unwrap();
    let source = source();
    let mut builder = WgpuRenderProgramBuilder::new(source, 1, GraphLimits::default()).unwrap();
    let world = builder.texture::<Color>(descriptor.clone()).unwrap();
    let final_color = builder.texture::<Color>(descriptor).unwrap();
    let texture = WgpuQuadTexture::Image(red);
    let sprite_capture = SpriteCapture {
        target: world.clone(),
        viewport,
        batches: vec![WgpuQuadBatch {
            material: WgpuQuadMaterialKey {
                texture: texture.key(),
                sampler: SamplerDescriptor::default(),
                alpha_mode: AlphaMode2d::Blend,
            },
            texture,
            scissor: None,
            counts_as_sprites: true,
            material_extension: None,
            instances: vec![WgpuQuadInstance {
                center: [0.0, 0.0],
                x_axis: [1.0, 0.0],
                y_axis: [0.0, 1.0],
                color: [1.0; 4],
                uv_min: [0.0, 0.0],
                uv_size: [1.0, 1.0],
                texture_mode: 0,
                shape_mode: WGPU_QUAD_SHAPE_PLAIN,
                rect_size: [0.0; 2],
                corner_radius: 0.0,
                border_width: 0.0,
            }],
        }]
        .into(),
    };
    let packet =
        super::tests::test_ui_image_packet(green, nara_render::RenderTarget::PrimaryWindow);
    let ui_capture = UiCapture::new(
        &[packet.clone()],
        nara_render::RenderTarget::Window(WindowId::PRIMARY),
        Some(WindowId::PRIMARY),
        final_color.clone(),
        viewport,
    )
    .unwrap();
    let facts = Arc::clone(&ui_capture.packet_facts);
    builder
        .operation(
            2,
            &ui,
            Arc::new(ui_capture),
            &[],
            &[final_color.preserve(&world, AccessRole::Attachment)],
        )
        .unwrap();
    builder
        .operation(
            8,
            &sprites,
            Arc::new(sprite_capture),
            &[],
            &[world.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder
        .finish(&[world.output(), final_color.output()])
        .unwrap();
    host.begin_frame(100).unwrap();
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(matches!(
        error,
        WgpuRenderHostError::Backend(WgpuRenderError::ContentRejected { .. })
    ));
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.backend.texture_stats().uploaded_images, 0);
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);
    assert_eq!(
        host.backend.stock_quad_resources().instance_charges(),
        (0, 0)
    );
    *host.backend.stock_quad_resources().textures_mut() =
        crate::texture::WgpuSpriteTextureCache::with_image_residency_limits(2, 1024);
    host = host.with_resource_byte_limit(512 * 1024);
    let outputs = host.submit_program(&producer, &program).unwrap();
    let sprite_report = outputs.report::<SpriteFeature<Color>>(8, &sprites).unwrap();
    assert_eq!(sprite_report.sprites, 1);
    let ui_report = outputs.report::<UiFeature<Color>>(2, &ui).unwrap();
    assert_eq!(ui_report.draw_calls, 1);
    assert_eq!(ui_report.sprites, 0);
    assert_eq!(ui_report.image_sprites, 0);
    assert!(Arc::ptr_eq(&facts, &ui_report.packet_facts));
    assert_eq!(ui_report.packet_facts[0].packet, packet.identity());
    let world_output = outputs.texture(&world).unwrap();
    let composite = outputs.texture(&final_color).unwrap();
    drain(outputs.submitted_prefix());
    let (reserved, committed) = host.backend.stock_quad_resources().instance_charges();
    assert_eq!(reserved, 0);
    assert!(committed > 0);
    host.begin_frame(101).unwrap();
    assert_eq!(
        host.backend.texture_stats().evicted_image_textures,
        0,
        "independently paced producer frame1 uses Host maintenance frame100 for cache age"
    );
    assert_eq!(host.backend.texture_stats().image_textures, 2);
    assert!(
        pixels(&mut host, &world_output)
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    let composed = pixels(&mut host, &composite);
    assert_eq!(composed[0], [0, 255, 0, 255]);
    assert_eq!(composed[63 * 64 + 63], [255, 0, 0, 255]);
    drop((outputs, world_output, composite));
    assert!(
        host.retained_resource_bytes() > 0,
        "the actual cached image allocations remain charged after the output releases"
    );
    while !host.poll_retirement().unwrap() {
        std::thread::yield_now();
    }
    assert_eq!(host.retained_resource_bytes(), 0);
}

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
    let row_bytes = texture.width() * 4;
    let row_stride = row_bytes.div_ceil(256) * 256;
    let readback = namespace.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("host-program-readback"),
        size: u64::from(texture.height()) * u64::from(row_stride),
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
                bytes_per_row: Some(row_stride),
                rows_per_image: Some(texture.height()),
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
            .chunks_exact(row_stride as usize)
            .flat_map(|row| {
                row[..row_bytes as usize]
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
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(2 * COLOR_BYTES - 1);
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
    assert_eq!(host.retained_resource_bytes(), 0);
    host = host.with_resource_byte_limit(2 * COLOR_BYTES);
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
    assert_eq!(host.retained_resource_bytes(), COLOR_BYTES);
    assert!(
        pixels(&mut host, &lease)
            .iter()
            .all(|pixel| *pixel == [0, 0, 255, 255])
    );
    drop(outputs);
    assert_eq!(host.retained_resource_bytes(), COLOR_BYTES);
    drop(lease);
    assert_eq!(host.retained_resource_bytes(), 0);
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
    assert_eq!(host.retained_resource_bytes(), 0);
    let outputs = host.submit_program(&producer, &program).unwrap();
    assert_eq!(outputs.report::<RejectOnce>(1, &instance).unwrap(), &());
    let output = outputs.texture(&output).unwrap();
    assert!(
        pixels(&mut host, &output)
            .iter()
            .all(|pixel| *pixel == [0, 0, 255, 255])
    );
}
