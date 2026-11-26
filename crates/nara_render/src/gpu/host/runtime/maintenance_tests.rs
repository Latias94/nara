use super::*;

#[test]
fn maintenance_without_a_device_stays_quiet_with_or_without_telemetry() {
    for telemetry in [false, true] {
        let mut host = WgpuRenderHost::offscreen();
        if telemetry {
            host.configure_telemetry(WgpuBackendTelemetryConfig::default());
        }
        let producer = host.create_producer().unwrap();
        let producer_id = producer.id();

        for _ in 0..2 {
            let maintenance = host.poll_runtime_maintenance();
            assert!(maintenance.error.is_none());
            assert!(maintenance.samples.is_empty());
            assert!(!maintenance.needs_poll);
            let facts = host.runtime_facts();
            assert_eq!(facts.device_epoch, 0);
            assert!(facts.adapter.is_none());
            assert_eq!(facts.completion.enabled(), telemetry);
            assert_eq!(facts.completion.pending_samples(), 0);
            assert_eq!(facts.resources.pending_retirements(), 0);
            assert!(host.views.contains_key(&producer_id));
        }

        drop(producer);
        let maintenance = host.poll_runtime_maintenance();
        assert!(maintenance.error.is_none());
        assert!(maintenance.samples.is_empty());
        assert!(!maintenance.needs_poll);
        assert!(host.views.is_empty());
        assert_eq!(host.retained_resource_bytes(), 0);
        assert!(host.poll_runtime_retirement(&[]).unwrap());
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
        sync::{Arc, mpsc},
        time::{Duration, Instant},
    };

    use crate::{Extent2d, RenderFrameSourceGeneration, RenderImage2d, RenderImagePacket};

    use super::*;
    use crate::gpu::{
        AccessRole, GraphLimits, TextureAlpha, TextureInterpretation, TextureTransfer,
        WgpuProgramTextureDescriptor, WgpuRenderProgramBuilder,
        submission::WgpuSubmissionReservation,
    };

    const TIMEOUT: Duration = Duration::from_secs(10);
    const APP_FRAME: u64 = 41;
    const RENDER_FRAME: u64 = 7;

    /// Retires the original Host before propagating an assertion failure or polling timeout.
    fn with_native_host(telemetry: bool, test: impl FnOnce(&mut WgpuRenderHost)) {
        let mut host = WgpuRenderHost::offscreen();
        if telemetry {
            host.configure_telemetry(WgpuBackendTelemetryConfig::default());
        }
        let outcome = catch_unwind(AssertUnwindSafe(|| test(&mut host)));
        let deadline = Instant::now() + TIMEOUT;
        while !host
            .poll_runtime_retirement(&[])
            .expect("the original test Host must remain retireable")
        {
            assert!(
                Instant::now() < deadline,
                "original Host retirement timed out"
            );
            std::thread::yield_now();
        }
        if let Err(failure) = outcome {
            resume_unwind(failure);
        }
    }

    fn poll_until_quiet(host: &mut WgpuRenderHost) -> Vec<WgpuFrameCompletionSample> {
        let deadline = Instant::now() + TIMEOUT;
        let mut samples = Vec::new();
        loop {
            let maintenance = host.poll_runtime_maintenance();
            assert!(
                maintenance.error.is_none(),
                "original Host maintenance failed: {:?}",
                maintenance.error
            );
            samples.extend(maintenance.samples);
            if !maintenance.needs_poll {
                assert!(host.retiring_prefixes.is_empty());
                let facts = host.runtime_facts();
                assert_eq!(facts.completion.pending_samples(), 0);
                assert_eq!(facts.completion.buffered_samples(), 0);
                assert_eq!(facts.resources.pending_retirements(), 0);
                return samples;
            }
            assert!(
                Instant::now() < deadline,
                "original maintenance did not settle"
            );
            std::thread::yield_now();
        }
    }

    fn assert_completion_evidence(
        host: &WgpuRenderHost,
        samples: &[WgpuFrameCompletionSample],
        telemetry: bool,
    ) {
        let facts = host.runtime_facts();
        assert_ne!(facts.device_epoch, 0);
        assert!(facts.adapter.is_some());
        assert_eq!(facts.completion.enabled(), telemetry);
        assert_eq!(facts.resources.tracking_enabled(), telemetry);
        assert_eq!(facts.completion.completed_samples(), u64::from(telemetry));
        assert_eq!(facts.completion.lost_samples(), 0);
        assert_eq!(facts.resources.lost_retirements(), 0);
        assert_eq!(samples.len(), usize::from(telemetry));
        if let Some(sample) = samples.first() {
            assert_eq!(sample.app_frame_index(), APP_FRAME);
            assert_eq!(sample.render_transaction_frame_index(), RENDER_FRAME);
            assert_eq!(sample.device_epoch(), facts.device_epoch);
        }
    }

    struct Color;

    fn color_descriptor() -> WgpuProgramTextureDescriptor {
        WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("runtime-maintenance-color"),
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
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            interpretation: TextureInterpretation::Color {
                transfer: TextureTransfer::Linear,
                alpha: TextureAlpha::Premultiplied,
            },
        }
    }

    /// Exercises offscreen submission and production telemetry registration without presentation.
    fn verify_program_settlement(telemetry: bool) {
        with_native_host(telemetry, |host| {
            let producer = host.create_producer().unwrap();
            host.begin_frame(RENDER_FRAME).unwrap();
            let source = RenderFrameSourceGeneration::try_acquire().unwrap();
            let mut builder =
                WgpuRenderProgramBuilder::new(source, RENDER_FRAME, GraphLimits::default())
                    .unwrap();
            let scratch = builder.texture::<Color>(color_descriptor()).unwrap();
            let output = builder.texture::<Color>(color_descriptor()).unwrap();
            builder
                .initialize(
                    1,
                    &[scratch.clear(wgpu::Color::BLUE, AccessRole::Attachment)],
                )
                .unwrap();
            builder
                .initialize(2, &[output.preserve(&scratch, AccessRole::Attachment)])
                .unwrap();
            let program = builder.finish(&[output.output()]).unwrap();
            let weak_program = Arc::downgrade(&program.0);
            let started = Instant::now();
            let outputs = host.submit_program(&producer, &program).unwrap();
            let prefix = outputs.submitted_prefix().clone();
            let weak_output = Arc::downgrade(&outputs.texture(&output).unwrap().storage);
            host.backend
                .track_runtime_submission(APP_FRAME, RENDER_FRAME, started)
                .unwrap();
            let transaction = host.frame_transaction_stats();
            assert_eq!(transaction.queue_submissions(), 1);
            assert_eq!(transaction.native_work().encoded_texture_copy_commands, 1);
            drop((outputs, program));

            let samples = poll_until_quiet(host);
            assert!(prefix.is_complete());
            assert!(!prefix.callback_abandoned());
            assert!(weak_program.upgrade().is_none());
            assert!(weak_output.upgrade().is_none());
            assert_eq!(host.retained_resource_bytes(), 0);
            assert_eq!(host.cleanup_fault_count(), 0);
            assert_eq!(host.abandoned_submission_count(), 0);
            assert_eq!(host.frame_transaction_stats(), transaction);
            assert!(host.views.contains_key(&producer.id()));
            assert_completion_evidence(host, &samples, telemetry);

            let quiet = host.poll_runtime_maintenance();
            assert!(quiet.error.is_none());
            assert!(!quiet.needs_poll);
            assert!(
                quiet.samples.is_empty(),
                "completion samples must be delivered once"
            );
            assert_eq!(host.frame_transaction_stats(), transaction);
        });
    }

    #[test]
    #[ignore = "requires a selected native WGPU adapter; acquisition or polling failure fails the test"]
    fn original_host_maintenance_settles_program_resources_without_telemetry() {
        verify_program_settlement(false);
    }

    #[test]
    #[ignore = "requires a selected native WGPU adapter; acquisition or polling failure fails the test"]
    fn original_host_maintenance_settles_program_resources_and_exact_telemetry() {
        verify_program_settlement(true);
    }

    #[test]
    #[ignore = "requires a selected native WGPU adapter and native command validation"]
    fn original_host_maintenance_settles_failed_native_uploads_without_losing_the_error() {
        for failure in 0..3 {
            with_native_host(true, |host| {
                host.begin_frame(RENDER_FRAME).unwrap();
                let namespace = host.prepare_namespace(None).unwrap();
                let source = namespace.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("runtime-maintenance-failed-upload"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                let baseline = host.retained_resource_bytes();
                let started = Instant::now();
                let error = host
                    .submit_image_use::<()>(&[], |device, queue, encoder, _| {
                        queue.write_buffer(&source, 0, &[37; 16]);
                        match failure {
                            0 => Err(WgpuRenderHostError::ImageUseCallbackFailed {
                                message: "refused after upload".to_owned(),
                            }),
                            1 => panic!("injected native callback panic after upload"),
                            _ => {
                                let invalid = device.create_buffer(&wgpu::BufferDescriptor {
                                    label: Some("runtime-maintenance-invalid-clear"),
                                    size: 16,
                                    usage: wgpu::BufferUsages::COPY_SRC,
                                    mapped_at_creation: false,
                                });
                                encoder.clear_buffer(&invalid, 0, None);
                                Ok(())
                            }
                        }
                    })
                    .unwrap_err();
                match failure {
                    0 => assert!(matches!(
                        error,
                        WgpuRenderHostError::ImageUseCallbackFailed { .. }
                    )),
                    1 => assert!(matches!(
                        error,
                        WgpuRenderHostError::NativeCallbackPanicked { stage: "image use" }
                    )),
                    _ => assert!(error.is_content_rejected(), "{error:?}"),
                }
                let prefix = host
                    .retiring_prefixes
                    .last()
                    .expect("the failed attempt must retain its original prefix")
                    .clone();
                host.backend
                    .track_runtime_submission(APP_FRAME, RENDER_FRAME, started)
                    .unwrap();

                let samples = poll_until_quiet(host);
                assert!(prefix.is_complete());
                assert!(!prefix.callback_abandoned());
                assert_eq!(host.retained_resource_bytes(), baseline);
                assert_eq!(host.cleanup_fault_count(), 0);
                assert_completion_evidence(host, &samples, true);

                let (readback, read_prefix) = host
                    .submit_image_use(&[], |device, _, encoder, _| {
                        let readback = device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("runtime-maintenance-failed-upload-readback"),
                            size: 16,
                            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                            mapped_at_creation: false,
                        });
                        encoder.copy_buffer_to_buffer(&source, 0, &readback, 0, 16);
                        Ok(readback)
                    })
                    .unwrap();
                let (sender, receiver) = mpsc::sync_channel(1);
                readback.map_async(wgpu::MapMode::Read, .., move |result| {
                    let _ = sender.send(result);
                });
                let deadline = Instant::now() + TIMEOUT;
                loop {
                    assert!(poll_until_quiet(host).is_empty());
                    match receiver.try_recv() {
                        Ok(result) => {
                            result.expect("the submitted upload readback must map successfully");
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => {
                            assert!(
                                Instant::now() < deadline,
                                "upload readback mapping timed out"
                            );
                            std::thread::yield_now();
                        }
                        Err(error) => panic!("upload readback callback disappeared: {error}"),
                    }
                }
                assert!(read_prefix.is_complete());
                assert_eq!(&*readback.get_mapped_range(..).unwrap(), &[37; 16]);
                readback.unmap();
                assert_eq!(host.retained_resource_bytes(), baseline);
            });
        }
    }

    #[test]
    #[ignore = "requires a selected native WGPU adapter; acquisition or polling failure fails the test"]
    fn original_prefix_release_reclaims_its_newly_closed_producer_before_quiet() {
        with_native_host(false, |host| {
            host.begin_frame(RENDER_FRAME).unwrap();
            let producer = host.create_producer().unwrap();
            let producer_id = producer.id();
            let source = RenderFrameSourceGeneration::try_acquire().unwrap();
            let target = RenderImagePacket::new(
                RenderImage2d::allocate_handle().unwrap(),
                Extent2d::new(4, 4).unwrap(),
            )
            .unwrap();
            let target = host
                .reserve_image_target(&producer, source, target)
                .unwrap();
            host.activate_image_target(&producer, target).unwrap();
            let namespace = host.prepare_namespace(None).unwrap();
            let buffer = namespace.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("runtime-maintenance-last-producer-upload"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let allocation = host.output_budget.reserve(16).unwrap();
            let retention = WgpuSubmissionReservation::new(
                &namespace.device,
                &namespace.queue,
                (producer, buffer.clone(), allocation),
            );
            let prefix = retention.observe();
            host.retiring_prefixes.push(prefix.clone());
            let submitted = catch_unwind(AssertUnwindSafe(|| {
                namespace.queue.write_buffer(&buffer, 0, &[91; 16]);
                let mut encoder = namespace
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                encoder.clear_buffer(&buffer, 8, None);
                namespace.queue.submit([encoder.finish()])
            }));
            match submitted {
                Ok(index) => {
                    let _ = retention.submitted(index);
                }
                Err(failure) => {
                    let _ = retention.bind_current_prefix();
                    resume_unwind(failure);
                }
            }
            drop((buffer, namespace));

            assert!(host.views.contains_key(&producer_id));
            assert!(host.retained_resource_bytes() > 0);
            assert!(poll_until_quiet(host).is_empty());
            assert!(prefix.is_complete());
            assert!(!prefix.callback_abandoned());
            assert!(!host.views.contains_key(&producer_id));
            assert_eq!(host.retained_resource_bytes(), 0);
            assert_eq!(host.cleanup_fault_count(), 0);
        });
    }
}
