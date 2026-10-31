use super::*;
use std::time::{Duration, Instant};

#[test]
fn producer_slot_storage_stays_bounded_on_a_native_callback_sized_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut host = WgpuRenderHost::offscreen();
            let mut producers = Vec::new();
            for _ in 0..32 {
                producers.push(host.create_producer().unwrap());
            }
            assert_eq!(host.views.len(), 32);
            for producer in &producers {
                assert!(host.retire_producer(producer).is_none());
            }
            assert!(host.views.is_empty());
            assert!(host.poll_retirement().unwrap());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn native_metadata_refusal_precedes_device_creation_and_close_precedes_capacity() {
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(0);
    let refused = host.submit_image_use::<()>(&[], |_, _, _, _| {
        panic!("refused metadata must not invoke native code")
    });
    assert!(matches!(refused, Err(WgpuRenderHostError::OutputCapacity)));
    assert_eq!(host.backend.state(), crate::WgpuBackendState::Uninitialized);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert!(host.retiring_prefixes.is_empty());
    assert!(host.poll_retirement().unwrap());
    assert!(matches!(
        host.submit_image_use::<()>(&[], |_, _, _, _| panic!(
            "closed Host must not invoke native code"
        )),
        Err(WgpuRenderHostError::Backend(
            WgpuRenderError::BackendUnavailable
        ))
    ));
}

#[test]
fn window_tables_refuse_before_device_work_and_refund_failed_admission() {
    let required = WindowPaintStorage::metadata_bytes(0).unwrap();
    let too_small = Arc::new(OutputBudget::new(required - 1));
    assert!(matches!(
        WindowPaintStorage::admit(&too_small, &[]),
        Err(WgpuRenderHostError::OutputCapacity)
    ));
    assert_eq!(too_small.retained(), 0);

    let budget = Arc::new(OutputBudget::new(required));
    let storage = WindowPaintStorage::admit(&budget, &[]).unwrap();
    assert_eq!(budget.retained(), required);
    assert!(storage.commands.capacity() >= MAX_NATIVE_USE_ITEMS);
    assert!(matches!(
        WindowPaintStorage::admit(&budget, &[]),
        Err(WgpuRenderHostError::OutputCapacity)
    ));
    assert_eq!(budget.retained(), required);
    drop(storage);
    assert_eq!(budget.retained(), 0);
    assert!(WindowPaintStorage::admit(&budget, &[]).is_ok());
    assert!(matches!(
        WindowPaintStorage::metadata_bytes(MAX_NATIVE_USE_ITEMS + 1),
        Err(WgpuRenderHostError::NativeUseLimit { .. })
    ));
}

#[test]
fn callback_unwind_is_an_attempt_fault_without_replacing_its_host() {
    let error = native_callback::<()>("test paint", || panic!("injected native paint callback"))
        .unwrap_err();
    assert!(matches!(
        error,
        WgpuRenderHostError::NativeCallbackPanicked {
            stage: "test paint"
        }
    ));
    assert!(!error.requires_owner_replacement());
    assert!(!error.is_deferred());
    assert_eq!(native_callback("test paint", || Ok(17)).unwrap(), 17);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure fails the prerequisite"]
fn native_use_admission_reclaims_closed_producer_storage_before_its_next_budget_check() {
    for window in [false, true] {
        let mut host = WgpuRenderHost::offscreen();
        host.begin_frame(1).unwrap();
        let producer = host.create_producer().unwrap();
        let source = RenderFrameSourceGeneration::try_acquire().unwrap();
        let target = RenderImagePacket::new(
            nara_render::RenderImage2d::allocate_handle().unwrap(),
            Extent2d::new(1, 1).unwrap(),
        )
        .unwrap();
        let reservation = host
            .reserve_image_target(&producer, source, target)
            .unwrap();
        host.activate_image_target(&producer, reservation).unwrap();
        let retained = host.retained_resource_bytes();
        assert!(
            retained > 0,
            "the closed producer still owns an actual native allocation"
        );
        drop(producer);
        assert_eq!(
            host.retained_resource_bytes(),
            retained,
            "producer Drop revokes admission; Host maintenance releases its target storage"
        );
        let metadata = if window {
            WindowPaintStorage::metadata_bytes(0).unwrap()
        } else {
            WgpuSubmissionReservation::modeled_metadata_bytes::<(
                Vec<WgpuImageOutput>,
                OutputReservation,
            )>() as u64
        };
        host = host.with_resource_byte_limit(metadata);
        if window {
            let packet = RenderWindowPacket {
                id: WindowId::PRIMARY,
                resolution: nara_window::WindowResolution::new(1, 1),
                present_mode: nara_window::PresentMode::AutoNoVsync,
            };
            let error = host
                .present_window(
                    packet,
                    1,
                    &[],
                    |_| panic!("a missing native window cannot invoke its paint callback"),
                    || {},
                )
                .unwrap_err();
            assert!(
                matches!(
                    error,
                    WgpuRenderHostError::Backend(WgpuRenderError::SurfaceMissing { .. })
                ),
                "reclaim before budgeting, then preserve the actual missing-window refusal: {error:?}"
            );
        } else {
            let (_, prefix) = host.submit_image_use(&[], |_, _, _, _| Ok(())).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !prefix.poll().unwrap() {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
        host.maintain_producers().unwrap();
        assert_eq!(host.retained_resource_bytes(), 0);
        assert!(host.views.is_empty());
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; native validation must execute"]
fn native_image_use_keeps_its_validation_error_when_an_unpublished_result_destructor_panics() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct Undelivered(Arc<AtomicUsize>);
    impl Drop for Undelivered {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("injected undelivered native result cleanup failure");
        }
    }

    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let error = host
        .submit_image_use(&[], |device, _, encoder, _| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("native-image-use-invalid-clear-source"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            // This is a real scoped native validation failure: clear requires COPY_DST.
            encoder.clear_buffer(&buffer, 0, None);
            Ok(Undelivered(Arc::clone(&drops)))
        })
        .unwrap_err();
    assert!(
        matches!(
            error,
            WgpuRenderHostError::Backend(WgpuRenderError::ContentRejected { .. })
        ),
        "the original scoped native failure must remain observable: {error:?}"
    );
    assert!(error.is_content_rejected());
    assert!(!error.requires_owner_replacement());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(host.cleanup_fault_count(), 1);
    let prefix = host.retiring_prefixes.last().unwrap().clone();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !prefix.poll().unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(!prefix.callback_abandoned());
    host.maintain_producers().unwrap();
    assert_eq!(host.cleanup_fault_count(), 1, "cleanup is counted once");
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; adapter failure fails the prerequisite"]
fn native_window_sample_dedup_and_command_limit_stop_growth_at_admission() {
    use nara_core::Mat3;
    use nara_render::{
        Color, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, RenderViewId,
        ViewLoadOp, ViewMapping2d, ViewportRect, build_image_render_frame_packet,
    };

    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let source = RenderFrameSourceGeneration::try_acquire().unwrap();
    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(1, 1).unwrap(),
    )
    .unwrap();
    let viewport = ViewportRect::new(0, 0, 1, 1).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: nara_ecs::Entity::PLACEHOLDER,
        target: RenderTarget::Image(target.image()),
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 1.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    let packet = build_image_render_frame_packet(source, 1, &views, target)
        .unwrap()
        .unwrap();
    let installation = crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::new())
        .install()
        .unwrap();
    let input = installation
        .capture(packet, Arc::new(crate::WgpuStockCapture::new()))
        .unwrap();
    let producer = host.create_producer().unwrap();
    let reservation = host
        .reserve_image_target(&producer, source, target)
        .unwrap();
    host.activate_image_target(&producer, reservation).unwrap();
    let output = host.submit(&producer, &input).unwrap();
    let display = host.display_image(output).unwrap();
    let mirror = host.display_image(display.output().clone()).unwrap();
    let displays = [display.clone(), mirror.clone()];
    let mut storage = WindowPaintStorage::admit(&host.output_budget, &displays).unwrap();
    let namespace = host.prepare_namespace(None).unwrap();
    let mut encoder = namespace
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut frame = WgpuWindowFrame {
        device: &namespace.device,
        queue: &namespace.queue,
        encoder: &mut encoder,
        target_view: &display.output().0.view,
        surface_format: display.output().format(),
        surface_extent: target.extent(),
        viewports: &displays,
        leading_command_buffers: &mut storage.commands,
        sampled_viewports: &mut storage.samples,
    };
    for _ in 0..10_000 {
        frame.record_image_sample(&display).unwrap();
    }
    frame.record_image_sample(&mirror).unwrap();
    assert_eq!(
        frame.sampled_viewports.len(),
        2,
        "mirrors differ while repainting one occurrence stays bounded"
    );
    let foreign = host.display_image(display.output().clone()).unwrap();
    assert!(matches!(
        frame.record_image_sample(&foreign),
        Err(WgpuRenderHostError::StaleImageSubmission)
    ));
    assert_eq!(frame.sampled_viewports.len(), 2);

    let created = std::cell::Cell::new(0);
    let commands = (0..usize::MAX).map(|_| {
        created.set(created.get() + 1);
        namespace
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
            .finish()
    });
    assert!(matches!(
        frame.prepend_command_buffers(commands),
        Err(WgpuRenderHostError::NativeUseLimit {
            kind: "window command buffers",
            ..
        })
    ));
    assert_eq!(
        created.get(),
        MAX_NATIVE_USE_ITEMS + 1,
        "reject during iteration without collecting the remaining input"
    );
    assert_eq!(frame.leading_command_buffers.len(), MAX_NATIVE_USE_ITEMS);
    let mut generation = u64::MAX - 1;
    let window = WgpuWindowPresentedReceipt::new(
        WindowId::PRIMARY,
        1,
        namespace.backend_instance_id,
        namespace.device_epoch,
        1,
    );
    assert!(matches!(
        join_composite_receipts_into(
            &mut generation,
            &displays,
            &storage.samples,
            window,
            &mut storage.composites
        ),
        Err(WgpuRenderHostError::PresentationGenerationExhausted)
    ));
    assert_eq!(
        generation,
        u64::MAX - 1,
        "a refused complete join cannot partially consume the shared generation"
    );
}

#[test]
#[ignore = "requires an explicitly selected native GPU; adapter or polling failure fails the prerequisite"]
fn native_failed_image_use_binds_queued_uploads_and_releases_its_original_metadata() {
    for panic in [false, true] {
        let mut host = WgpuRenderHost::offscreen();
        host.begin_frame(1).unwrap();
        let namespace = host.prepare_namespace(None).unwrap();
        let baseline = host.retained_resource_bytes();
        let source = namespace.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("native-failed-image-use-upload"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let failure = host
            .submit_image_use::<()>(&[], |_, queue, _, _| {
                queue.write_buffer(&source, 0, &[37; 16]);
                if panic {
                    panic!("injected callback failure after a real upload");
                }
                Err(WgpuRenderHostError::window_paint(
                    "injected read-use refusal",
                ))
            })
            .unwrap_err();
        assert!(if panic {
            matches!(
                failure,
                WgpuRenderHostError::NativeCallbackPanicked { stage: "image use" }
            )
        } else {
            matches!(failure, WgpuRenderHostError::WindowPaint { .. })
        });
        let original = host
            .retiring_prefixes
            .last()
            .expect("failure retains an observable original prefix")
            .clone();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !original.poll().unwrap() {
            assert!(
                Instant::now() < deadline,
                "the failed attempt's queued uploads did not settle"
            );
            std::thread::yield_now();
        }
        assert!(!original.callback_abandoned());
        host.maintain_producers().unwrap();
        assert_eq!(
            host.retained_resource_bytes(),
            baseline,
            "no unbound custody may retain the metadata forever"
        );

        let (readback, prefix) = host
            .submit_image_use(&[], |device, _, encoder, _| {
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("native-failed-image-use-readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_buffer_to_buffer(&source, 0, &readback, 0, 16);
                Ok(readback)
            })
            .unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        namespace
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert!(prefix.is_complete());
        assert_eq!(&readback.get_mapped_range(..).unwrap()[..], &[37; 16]);
        readback.unmap();
        host.maintain_producers().unwrap();
        assert_eq!(host.retained_resource_bytes(), baseline);
        assert_eq!(host.cleanup_fault_count(), 0);
    }
}
