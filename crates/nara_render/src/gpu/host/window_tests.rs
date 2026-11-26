use super::*;
use std::time::{Duration, Instant};

#[cfg(windows)]
mod native;

#[test]
fn native_frontend_requirements_do_not_acquire_and_closed_host_refuses_them() {
    let mut host = WgpuRenderHost::offscreen();
    let base = crate::gpu::WgpuDeviceRequirements::new(
        wgpu::Features::TIMESTAMP_QUERY,
        wgpu::Limits::default(),
    );
    let encoder_queries = crate::gpu::WgpuDeviceRequirements::new(
        wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
        wgpu::Limits::default(),
    );
    host.request_device_requirements(&base).unwrap();
    host.request_device_requirements(&encoder_queries).unwrap();
    assert_eq!(
        host.backend.state(),
        crate::gpu::WgpuBackendState::Uninitialized
    );
    assert_eq!(host.device_epoch(), 0);
    assert_eq!(host.frame_transaction_stats(), Default::default());

    let experimental = crate::gpu::WgpuDeviceRequirements::new(
        wgpu::Features::all_experimental_mask(),
        wgpu::Limits::default(),
    );
    assert!(matches!(
        host.request_device_requirements(&experimental),
        Err(WgpuRenderHostError::DeviceRequirements(
            crate::gpu::WgpuDeviceRequirementsError::ExperimentalFeaturesUnsupported { .. }
        ))
    ));
    assert_eq!(host.device_epoch(), 0);
    host.request_device_requirements(&base).unwrap();
    assert!(host.poll_retirement().unwrap());
    assert!(matches!(
        host.request_device_requirements(&experimental),
        Err(WgpuRenderHostError::Backend(
            WgpuRenderError::BackendUnavailable
        ))
    ));
}

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
    assert_eq!(
        host.backend.state(),
        crate::gpu::WgpuBackendState::Uninitialized
    );
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
fn callback_unwind_is_an_attempt_fault_without_replacing_its_host() {
    let error = native_callback::<()>("image use", || panic!("injected native image-use callback"))
        .unwrap_err();
    assert!(matches!(
        error,
        WgpuRenderHostError::NativeCallbackPanicked { stage: "image use" }
    ));
    assert!(!error.requires_owner_replacement());
    assert!(!error.is_deferred());
    assert_eq!(native_callback("image use", || Ok(17)).unwrap(), 17);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure fails the prerequisite"]
fn native_use_admission_reclaims_closed_producer_storage_before_its_next_budget_check() {
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let producer = host.create_producer().unwrap();
    let source = RenderFrameSourceGeneration::try_acquire().unwrap();
    let target = RenderImagePacket::new(
        crate::RenderImage2d::allocate_handle().unwrap(),
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
    let metadata = WgpuSubmissionReservation::modeled_metadata_bytes::<(
        Vec<WgpuImageOutput>,
        OutputReservation,
    )>() as u64;
    host = host.with_resource_byte_limit(metadata);
    let (_, prefix) = host.submit_image_use(&[], |_, _, _, _| Ok(())).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !prefix.poll().unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    host.maintain_producers().unwrap();
    assert_eq!(host.retained_resource_bytes(), 0);
    assert!(host.views.is_empty());
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
                Err(WgpuRenderHostError::ImageUseCallbackFailed {
                    message: "injected read-use refusal".to_owned(),
                })
            })
            .unwrap_err();
        assert!(if panic {
            matches!(
                failure,
                WgpuRenderHostError::NativeCallbackPanicked { stage: "image use" }
            )
        } else {
            matches!(failure, WgpuRenderHostError::ImageUseCallbackFailed { .. })
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
