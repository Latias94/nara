use super::*;

fn stock_policy() -> &'static crate::WgpuPolicyInstallation<crate::WgpuStockPolicy> {
    static POLICY: std::sync::OnceLock<crate::WgpuPolicyInstallation<crate::WgpuStockPolicy>> =
        std::sync::OnceLock::new();
    POLICY.get_or_init(|| {
        crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::new())
            .install()
            .unwrap()
    })
}

pub(super) fn test_stock_input(packet: RenderFramePacket) -> crate::WgpuRenderInput {
    test_stock_capture(packet, crate::WgpuStockCapture::new())
}

pub(super) fn test_stock_capture(
    packet: RenderFramePacket,
    capture: crate::WgpuStockCapture,
) -> crate::WgpuRenderInput {
    stock_policy().capture(packet, Arc::new(capture)).unwrap()
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_direct_host_drop_drains_the_original_callback_cycle() {
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let producer = host.create_producer().unwrap();
    let packet = test_image_frame();
    let target = image_target(&packet);
    let reservation = host
        .reserve_image_target(&producer, packet.source_generation(), target)
        .unwrap();
    host.activate_image_target(&producer, reservation).unwrap();
    let output = host.submit(&producer, &test_stock_input(packet)).unwrap();
    let weak_output = Arc::downgrade(&output.0);
    drop(output);
    drop(producer);
    drop(host);
    assert!(
        weak_output.upgrade().is_none(),
        "direct Host Drop must settle its native callbacks and output budget"
    );
}

#[test]
#[ignore = "requires an explicitly selected native GPU and a fail-stop subprocess"]
fn native_direct_host_drop_callback_panic_cannot_release_incomplete_dependencies() {
    const CHILD: &str = "NARA_RENDER_HOST_DROP_PANIC_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let mut host = WgpuRenderHost::offscreen();
        host.begin_frame(1).unwrap();
        let producer = host.create_producer().unwrap();
        let packet = test_image_frame();
        let target = image_target(&packet);
        let reservation = host
            .reserve_image_target(&producer, packet.source_generation(), target)
            .unwrap();
        host.activate_image_target(&producer, reservation).unwrap();
        let output = host.submit(&producer, &test_stock_input(packet)).unwrap();
        let (readback, _) = host
            .submit_image_use(
                std::slice::from_ref(&output),
                |device, _, encoder, views| {
                    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("host-drop-panicking-map"),
                        size: 256 * 64,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    });
                    encoder.copy_texture_to_buffer(
                        views[0].texture().as_image_copy(),
                        wgpu::TexelCopyBufferInfo {
                            buffer: &buffer,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(256),
                                rows_per_image: Some(64),
                            },
                        },
                        wgpu::Extent3d {
                            width: 64,
                            height: 64,
                            depth_or_array_layers: 1,
                        },
                    );
                    Ok(buffer)
                },
            )
            .unwrap();
        readback.map_async(wgpu::MapMode::Read, .., |result| {
            result.unwrap();
            eprintln!("CHILD_NATIVE_CALLBACK_PANIC");
            panic!("injected callback panic during Host Drop");
        });
        eprintln!("CHILD_HOST_READY");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(host)));
        eprintln!("DROP_RELEASED_INCOMPLETE_OWNER");
        std::process::exit(91);
    }
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "host::tests::native_direct_host_drop_callback_panic_cannot_release_incomplete_dependencies", "--ignored", "--nocapture"])
        .env(CHILD, "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let result = command.output().unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("CHILD_HOST_READY"),
        "native child prerequisite failed: {stderr}"
    );
    assert!(
        stderr.contains("CHILD_NATIVE_CALLBACK_PANIC"),
        "the callback never exercised Drop: {stderr}"
    );
    assert!(
        !stderr.contains("DROP_RELEASED_INCOMPLETE_OWNER"),
        "Drop unwound and released its unresolved native owner: {stderr}"
    );
    assert!(!result.status.success());
    assert_ne!(result.status.code(), Some(91));
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_released_reconstruction_input_is_unavailable_while_pixels_remain() {
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let producer = host.create_producer().unwrap();
    let packet = test_image_frame();
    let target = image_target(&packet);
    let reservation = host
        .reserve_image_target(&producer, packet.source_generation(), target)
        .unwrap();
    host.activate_image_target(&producer, reservation).unwrap();
    let input = Arc::new(test_stock_input(packet));
    let recovery = Arc::downgrade(&input);
    let output = host.submit(&producer, &input).unwrap();
    drain_submission_prefix(
        host.views[&producer.id()]
            .submitted_prefix
            .as_ref()
            .unwrap(),
    );
    drop(input);
    assert!(
        recovery.upgrade().is_none(),
        "the output cannot secretly retain retired reconstruction input"
    );
    assert!(output.is_current());
    assert_eq!(
        read_output_pixel(&mut host, &output, 16, 16),
        [0, 0, 0, 255]
    );
    // Reconstruction requires the original input owner; no output operation can recapture it
    // from live stores or manufacture the absent CPU facts. Pixel leases remain independent.
    drain_submission_prefix(&host.retire_producer(&producer).unwrap());
}
use nara_ecs::Entity;
use nara_render::{
    Color, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, ViewportRect,
    build_image_render_frame_packet,
};
#[cfg(feature = "stock-ui")]
use nara_ui_render::__private::UiBackendSubmission;
#[cfg(feature = "stock-ui")]
use nara_ui_render::{
    UiPaintPacket, UiPresentationAttemptId, UiPresentationCapabilityGeneration,
    UiPresentationEpoch, UiPresentationOutcome, UiPresentationRuntimeGeneration,
};

#[test]
fn viewport_allocation_failure_preserves_publication_and_smaller_retry() {
    let mut execution = WgpuRenderHost::offscreen();
    let Some(namespace) = test_namespace(&mut execution) else {
        return;
    };
    let frame = test_image_frame();
    let target = image_target(&frame);
    let source = frame.source_generation();
    let producer = execution.create_producer().unwrap();
    let view_id = producer.id();
    let initial = execution
        .reserve_image_target(&producer, source, target)
        .unwrap();
    let initial_stamp = initial.stamp;
    execution.activate_image_target(&producer, initial).unwrap();
    let receipt = WgpuSubmittedImageReceipt::new(
        view_id,
        target,
        initial_stamp.generation,
        source,
        initial_stamp.source_handoff_generation,
        namespace.backend_instance_id,
        namespace.device_epoch,
        7,
    );
    let output = allocated_test_output(&execution, &namespace, receipt);
    let retained_bytes = execution.retained_resource_bytes();
    let larger = nara_render::__private::editor_viewport_image_target(
        target.image(),
        Extent2d::new(128, 128).unwrap(),
    )
    .unwrap();
    // Check both texture allocation and view allocation failures. Neither candidate may
    // consume a generation, replace the live target or create a new registered view.
    for failure_stage in [1, 2] {
        VIEWPORT_ALLOCATION_FAILURE.with(|failure| failure.set(Some(failure_stage)));
        let error = execution
            .reserve_image_target(&producer, source, larger)
            .unwrap_err();
        assert!(error.is_deferred());
        assert!(!error.is_content_rejected());
        assert!(!error.requires_owner_replacement());
        assert_eq!(execution.views.len(), 1);
        assert_eq!(
            execution.views[&view_id].target.as_ref().unwrap().stamp,
            initial_stamp
        );
        assert_eq!(
            execution.views[&view_id].admission.last_target_generation,
            1
        );
        assert!(
            execution.views[&view_id]
                .target
                .as_ref()
                .unwrap()
                .initial_storage
                .is_some()
        );
        assert_eq!(execution.retained_resource_bytes(), retained_bytes);
        execution
            .validate_viewport_submission(&namespace, &output)
            .unwrap();
    }
    let unregistered = WgpuRenderProducerId::new(0);
    VIEWPORT_ALLOCATION_FAILURE.with(|failure| failure.set(Some(1)));
    assert!(
        execution
            .reserve_viewport_target_in_namespace(unregistered, &namespace, source, larger,)
            .is_err()
    );
    assert!(!execution.views.contains_key(&unregistered));
    assert_eq!(execution.retained_resource_bytes(), retained_bytes);

    let smaller = nara_render::__private::editor_viewport_image_target(
        target.image(),
        Extent2d::new(32, 32).unwrap(),
    )
    .unwrap();
    let retry = execution
        .reserve_image_target(&producer, source, smaller)
        .unwrap();
    assert_eq!(retry.stamp.generation, initial_stamp.generation + 1);
    execution
        .validate_viewport_submission(&namespace, &output)
        .unwrap();
    execution.retire_image_target(&producer);
    execution.activate_image_target(&producer, retry).unwrap();
    assert_eq!(
        execution.views[&view_id]
            .target
            .as_ref()
            .unwrap()
            .stamp
            .target,
        smaller
    );
    execution
        .validate_viewport_submission(&namespace, &output)
        .unwrap();
    assert!(execution.retire_producer(&producer).is_none());
    assert!(execution.views.is_empty());
    execution
        .validate_viewport_submission(&namespace, &output)
        .unwrap();
}

#[test]
fn editor_independent_producers_keep_their_own_admission_clock() {
    let scene = test_image_frame().source_generation();
    let game = test_image_frame().source_generation();
    let mut execution = WgpuRenderHost::offscreen();
    let scene_view = WgpuRenderProducerId::new(1);
    let game_view = WgpuRenderProducerId::new(2);
    let target = image_target(&test_image_frame());
    activate_test_view(&mut execution, scene_view, scene, target);
    execution
        .views
        .get_mut(&scene_view)
        .unwrap()
        .admission
        .admit(scene, 1)
        .unwrap();
    // Admission validates only. These fixtures commit successful frames explicitly so the
    // bounds below do not depend on a hidden consumption side effect.
    assert_eq!(
        execution.views[&scene_view].admission.last_frame_index,
        None
    );
    execution
        .views
        .get_mut(&scene_view)
        .unwrap()
        .admission
        .last_frame_index = Some(1);
    activate_test_view(&mut execution, game_view, game, target);
    execution
        .views
        .get_mut(&game_view)
        .unwrap()
        .admission
        .admit(game, 1)
        .unwrap();
    assert_eq!(execution.views[&game_view].admission.last_frame_index, None);
    execution
        .views
        .get_mut(&game_view)
        .unwrap()
        .admission
        .last_frame_index = Some(1);

    assert!(matches!(
        execution
            .views
            .get_mut(&game_view)
            .unwrap()
            .admission
            .admit(game, 1),
        Err(WgpuRenderError::FrameAlreadyConsumed { frame_index: 1 })
    ));
    execution
        .views
        .get_mut(&scene_view)
        .unwrap()
        .admission
        .admit(scene, 2)
        .expect("a Game submission must not invalidate the independent Scene producer");
    assert_eq!(
        execution.views[&scene_view].admission.last_frame_index,
        Some(1)
    );
    execution
        .views
        .get_mut(&scene_view)
        .unwrap()
        .admission
        .last_frame_index = Some(2);
    assert!(matches!(
        execution
            .views
            .get_mut(&scene_view)
            .unwrap()
            .admission
            .admit(scene, 2),
        Err(WgpuRenderError::FrameAlreadyConsumed { frame_index: 2 })
    ));
    execution
        .views
        .get_mut(&game_view)
        .unwrap()
        .admission
        .admit(game, 2)
        .unwrap();
}

#[test]
fn editor_host_transaction_accumulates_all_image_submissions() {
    let mut backend = WgpuRenderBackend::default();
    backend.begin_frame_transaction(23, None);
    backend.record_editor_image_submission();
    backend.record_editor_image_submission();

    let facts = backend.frame_transaction_stats();
    assert_eq!(facts.render_transaction_frame_index(), Some(23));
    assert_eq!(facts.packet_admissions(), 2);
    assert_eq!(facts.queue_submissions(), 2);
}

#[test]
fn independent_reservations_survive_another_view_activation_and_retirement() {
    let mut execution = WgpuRenderHost::offscreen();
    let scene = test_image_frame();
    let game = test_image_frame();
    let scene_producer = execution.create_producer().unwrap();
    let game_producer = execution.create_producer().unwrap();
    let scene_view = scene_producer.id();
    let game_view = game_producer.id();
    let namespace = (
        execution.backend.instance_id(),
        execution.backend.device_epoch(),
    );
    let reserve = |execution: &WgpuRenderHost, view_id, packet: &RenderFramePacket| {
        execution
            .next_viewport_reservation_stamp_for_namespace(
                view_id,
                namespace.0,
                namespace.1,
                packet.source_generation(),
                image_target(packet),
            )
            .unwrap()
    };
    let scene_reservation = reserve(&execution, scene_view, &scene);
    let game_reservation = reserve(&execution, game_view, &game);
    execution
        .views
        .entry(scene_view)
        .or_default()
        .admission
        .activate(scene_reservation);

    assert_eq!(reserve(&execution, game_view, &game), game_reservation);
    execution
        .views
        .entry(game_view)
        .or_default()
        .admission
        .activate(game_reservation);
    execution
        .views
        .get_mut(&game_view)
        .unwrap()
        .admission
        .admit(game.source_generation(), 1)
        .unwrap();
    execution
        .views
        .get_mut(&game_view)
        .unwrap()
        .admission
        .last_frame_index = Some(1);
    let next_game = reserve(&execution, game_view, &game);
    execution.retire_image_target(&scene_producer);
    assert_eq!(reserve(&execution, game_view, &game), next_game);
    assert_eq!(
        execution.views[&game_view].admission.last_frame_index,
        Some(1)
    );
    assert_eq!(
        next_game.source_handoff_generation,
        game_reservation.source_handoff_generation
    );
    assert_eq!(next_game.generation, game_reservation.generation + 1);
}

#[test]
fn closing_non_reused_views_releases_their_admission_metadata() {
    let mut execution = WgpuRenderHost::offscreen();
    let frame = test_image_frame();
    let target = image_target(&frame);
    let active = execution.create_producer().unwrap();
    activate_test_view(
        &mut execution,
        active.id(),
        frame.source_generation(),
        target,
    );
    for _ in 0..1024 {
        let closed = execution.create_producer().unwrap();
        assert_ne!(closed.id(), active.id());
        activate_test_view(
            &mut execution,
            closed.id(),
            frame.source_generation(),
            target,
        );
        assert!(execution.retire_producer(&closed).is_none());
    }
    assert_eq!(execution.views.len(), 1);
    assert!(execution.views.contains_key(&active.id()));
}

#[test]
fn closed_producer_cannot_reserve_or_resubmit_after_a_successor_is_created() {
    let mut execution = WgpuRenderHost::offscreen();
    let producer = execution.create_producer().unwrap();
    let closed_id = producer.id();
    let input = test_stock_input(test_image_frame());
    let source = input.packet().source_generation();
    let target = image_target(input.packet());

    assert!(execution.retire_producer(&producer).is_none());
    assert!(producer.closed.load(Ordering::Acquire));
    assert!(execution.views.is_empty());
    assert!(matches!(
        execution.reserve_image_target(&producer, source, target),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(matches!(
        execution.submit(&producer, &input),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(execution.views.is_empty());

    let successor = execution.create_producer().unwrap();
    assert_ne!(successor.id(), closed_id);
    assert!(matches!(
        execution.reserve_image_target(&producer, source, target),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(matches!(
        execution.submit(&producer, &input),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(!execution.image_target_matches(&producer, source, target));
    assert!(execution.retire_producer(&producer).is_none());
    assert_eq!(execution.views.len(), 1);
    assert!(execution.views.contains_key(&successor.id()));
    assert!(!execution.views.contains_key(&closed_id));
}

#[test]
fn resizing_one_view_preserves_both_producer_admission_and_other_view_identity() {
    let mut execution = WgpuRenderHost::offscreen();
    let scene = test_image_frame();
    let game = test_image_frame();
    let scene_view = WgpuRenderProducerId::new(1);
    let game_view = WgpuRenderProducerId::new(2);
    let first = activate_test_view(
        &mut execution,
        scene_view,
        scene.source_generation(),
        image_target(&scene),
    );
    let game_stamp = activate_test_view(
        &mut execution,
        game_view,
        game.source_generation(),
        image_target(&game),
    );
    execution
        .views
        .get_mut(&scene_view)
        .unwrap()
        .admission
        .admit(scene.source_generation(), 1)
        .unwrap();
    execution
        .views
        .get_mut(&scene_view)
        .unwrap()
        .admission
        .last_frame_index = Some(1);
    let resized_target = nara_render::__private::editor_viewport_image_target(
        image_target(&scene).image(),
        Extent2d::new(128, 96).unwrap(),
    )
    .unwrap();
    let resized = activate_test_view(
        &mut execution,
        scene_view,
        scene.source_generation(),
        resized_target,
    );

    assert_eq!(resized.generation, first.generation + 1);
    assert_eq!(
        resized.source_handoff_generation,
        first.source_handoff_generation
    );
    assert!(matches!(
        execution
            .views
            .get_mut(&scene_view)
            .unwrap()
            .admission
            .admit(scene.source_generation(), 1),
        Err(WgpuRenderError::FrameAlreadyConsumed { frame_index: 1 })
    ));
    let game_after_resize = &execution.views[&game_view].admission;
    assert_eq!(
        game_after_resize.last_target_generation,
        game_stamp.generation
    );
    assert_eq!(
        game_after_resize.source_generation,
        Some(game.source_generation())
    );
    assert_eq!(game_after_resize.last_frame_index, None);
}

#[test]
fn multiple_composites_certify_only_exact_sampled_images_in_one_window() {
    let mut execution = WgpuRenderHost::offscreen();
    let Some(namespace) = test_namespace(&mut execution) else {
        return;
    };
    let frame = test_image_frame();
    let target = image_target(&frame);
    let first = WgpuSubmittedImageReceipt::new(
        WgpuRenderProducerId::new(1),
        target,
        1,
        frame.source_generation(),
        1,
        21,
        4,
        1,
    );
    let second = WgpuSubmittedImageReceipt::new(
        WgpuRenderProducerId::new(2),
        target,
        1,
        frame.source_generation(),
        1,
        21,
        4,
        1,
    );
    let unsampled = WgpuSubmittedImageReceipt::new(
        WgpuRenderProducerId::new(3),
        target,
        1,
        frame.source_generation(),
        1,
        21,
        4,
        1,
    );
    let foreign = WgpuSubmittedImageReceipt::new(
        WgpuRenderProducerId::new(4),
        target,
        1,
        frame.source_generation(),
        1,
        21,
        4,
        1,
    );
    let first_display =
        WgpuImageDisplay::issue(allocated_test_output(&execution, &namespace, first));
    let mirror = WgpuImageDisplay::issue(first_display.output().clone());
    let second_display =
        WgpuImageDisplay::issue(allocated_test_output(&execution, &namespace, second));
    let unsampled_display =
        WgpuImageDisplay::issue(allocated_test_output(&execution, &namespace, unsampled));
    let foreign_display =
        WgpuImageDisplay::issue(allocated_test_output(&execution, &namespace, foreign));
    assert_eq!(mirror.receipt(), first_display.receipt());
    assert_ne!(mirror.occurrence(), first_display.occurrence());
    assert_eq!(
        first_display.clone().occurrence(),
        first_display.occurrence()
    );
    let window = WgpuEditorWindowPresentedReceipt::new(WindowId::PRIMARY, 3, 21, 4, 19);
    let mut generation = 0;
    let composites = join_composite_receipts(
        &mut generation,
        &[
            first_display.clone(),
            second_display.clone(),
            unsampled_display,
            mirror.clone(),
        ],
        &[
            ImageSample {
                image: second,
                occurrence: second_display.occurrence(),
            },
            ImageSample {
                image: first,
                occurrence: first_display.occurrence(),
            },
            ImageSample {
                image: foreign,
                occurrence: foreign_display.occurrence(),
            },
            ImageSample {
                image: second,
                occurrence: first_display.occurrence(),
            },
            ImageSample {
                image: first,
                occurrence: mirror.occurrence(),
            },
            ImageSample {
                image: first,
                occurrence: first_display.occurrence(),
            },
        ],
        window,
    )
    .unwrap();

    assert_eq!(composites.len(), 3);
    assert_eq!(composites[0].image(), second);
    assert_eq!(composites[1].image(), first);
    assert_eq!(composites[2].image(), first);
    assert_eq!(composites[0].occurrence(), second_display.occurrence());
    assert_eq!(composites[1].occurrence(), first_display.occurrence());
    assert_eq!(composites[2].occurrence(), mirror.occurrence());
    assert!(
        composites
            .iter()
            .all(|composite| composite.window() == window)
    );
    assert_eq!(composites[0].presentation_generation(), 1);
    assert_eq!(composites[1].presentation_generation(), 2);
    assert_eq!(composites[2].presentation_generation(), 3);
    assert_eq!(generation, 3);
}

#[test]
fn viewport_identity_does_not_authenticate_another_view_with_equal_generations() {
    let frame = test_image_frame();
    let target = image_target(&frame);
    let stamp = next_viewport_reservation_stamp(
        WgpuRenderProducerId::new(1),
        0,
        0,
        None,
        21,
        4,
        frame.source_generation(),
        target,
    )
    .unwrap();
    let receipt = WgpuSubmittedImageReceipt::new(
        WgpuRenderProducerId::new(2),
        target,
        stamp.generation,
        frame.source_generation(),
        stamp.source_handoff_generation,
        21,
        4,
        1,
    );

    assert!(!WgpuImageTargetIdentity { stamp }.matches_submission(receipt));
}

#[test]
fn composite_receipt_requires_the_exact_submitted_sample_and_window_namespace() {
    let mut execution = WgpuRenderHost::offscreen();
    let Some(namespace) = test_namespace(&mut execution) else {
        return;
    };
    let frame = test_image_frame();
    let image = image_target(&frame);
    let submitted = WgpuSubmittedImageReceipt::new(
        WgpuRenderProducerId::new(1),
        image,
        4,
        frame.source_generation(),
        6,
        10,
        2,
        7,
    );
    let window = WgpuEditorWindowPresentedReceipt::new(
        WindowId::PRIMARY,
        3,
        submitted.backend_instance_id(),
        submitted.device_epoch(),
        9,
    );
    let mut presentation_generation = 0;

    assert_eq!(
        join_composite_receipt(&mut presentation_generation, Some(submitted), None, window,)
            .unwrap(),
        None,
    );
    assert_eq!(presentation_generation, 0);

    let foreign_window = WgpuEditorWindowPresentedReceipt::new(
        WindowId::PRIMARY,
        3,
        submitted.backend_instance_id().saturating_add(1),
        submitted.device_epoch(),
        9,
    );
    let display = WgpuImageDisplay::issue(allocated_test_output(&execution, &namespace, submitted));
    let sample = ImageSample {
        image: submitted,
        occurrence: display.occurrence(),
    };
    assert_eq!(
        join_composite_receipt(
            &mut presentation_generation,
            Some(submitted),
            Some(sample),
            foreign_window,
        )
        .unwrap(),
        None,
    );
    assert_eq!(presentation_generation, 0);

    let foreign_epoch = WgpuEditorWindowPresentedReceipt::new(
        WindowId::PRIMARY,
        3,
        submitted.backend_instance_id(),
        submitted.device_epoch() + 1,
        9,
    );
    assert_eq!(
        join_composite_receipt(
            &mut presentation_generation,
            Some(submitted),
            Some(sample),
            foreign_epoch,
        )
        .unwrap(),
        None,
    );
    let same_configuration = WgpuSubmittedImageReceipt::new(
        submitted.producer_id(),
        submitted.target(),
        submitted.target_generation(),
        submitted.source_generation(),
        submitted.source_handoff_generation(),
        submitted.backend_instance_id(),
        submitted.device_epoch(),
        submitted.render_transaction_frame_index(),
    );
    assert_ne!(
        same_configuration.output_version(),
        submitted.output_version()
    );
    assert_eq!(
        join_composite_receipt(
            &mut presentation_generation,
            Some(same_configuration),
            Some(sample),
            window,
        )
        .unwrap(),
        None,
    );
    assert_eq!(
        join_composite_receipt(&mut presentation_generation, None, Some(sample), window).unwrap(),
        None,
    );
    assert_eq!(presentation_generation, 0);

    let composite = join_composite_receipt(
        &mut presentation_generation,
        Some(submitted),
        Some(sample),
        window,
    )
    .unwrap()
    .unwrap();
    assert_eq!(composite.presentation_generation(), 1);
    assert_eq!(composite.image(), submitted);
    assert_eq!(composite.occurrence(), display.occurrence());
    assert_eq!(
        composite.image().source_generation(),
        frame.source_generation()
    );
    assert_eq!(composite.image().source_handoff_generation(), 6);
    assert_eq!(presentation_generation, 1);
}

#[test]
fn viewport_source_handoff_advances_only_when_the_producer_changes() {
    let scene = test_image_frame().source_generation();
    let successor = test_image_frame().source_generation();

    assert_eq!(next_source_handoff_generation(0, None, scene).unwrap(), 1);
    assert_eq!(
        next_source_handoff_generation(1, Some(scene), scene).unwrap(),
        1
    );
    assert_eq!(
        next_source_handoff_generation(1, Some(scene), successor).unwrap(),
        2
    );
}

#[test]
fn stale_inactive_viewport_reservation_cannot_replace_a_committed_target() {
    let mut execution = WgpuRenderHost::offscreen();
    let Some(namespace) = test_namespace(&mut execution) else {
        return;
    };
    let first_frame = test_image_frame();
    let source = first_frame.source_generation();
    let first_target = image_target(&first_frame);
    let producer = execution.create_producer().unwrap();
    let view_id = producer.id();
    let first = execution
        .reserve_image_target(&producer, source, first_target)
        .unwrap();
    let duplicate = execution
        .reserve_image_target(&producer, source, first_target)
        .unwrap();
    let first_stamp = first.stamp;
    assert_eq!(duplicate.stamp, first_stamp);
    assert_eq!(execution.views.len(), 1);
    assert!(
        execution.views[&view_id].target.is_none(),
        "inactive reservations do not publish targets"
    );
    let original_host = std::ptr::from_ref(&execution);
    let activation = execution
        .prepare_image_target_activation(&producer, first)
        .unwrap();
    assert!(std::ptr::eq(
        std::ptr::from_ref(&*activation.host),
        original_host
    ));
    assert!(std::ptr::eq(activation._producer, &producer));
    assert!(activation.host.views[&view_id].target.is_none());
    assert_eq!(activation.identity().stamp, first_stamp);
    let () = activation.commit();
    assert!(matches!(
        execution.prepare_image_target_activation(&producer, duplicate),
        Err(WgpuRenderHostError::StaleViewportReservation)
    ));
    assert_eq!(
        execution.views[&view_id].target.as_ref().unwrap().stamp,
        first_stamp
    );
    let resized_target =
        RenderImagePacket::new(first_target.image(), Extent2d::new(128, 64).unwrap()).unwrap();
    let successor = execution
        .reserve_image_target(&producer, source, resized_target)
        .unwrap();

    assert_ne!(first_stamp, successor.stamp);
    assert_eq!(successor.stamp.generation, first_stamp.generation + 1);
    assert_eq!(
        successor.stamp.source_handoff_generation,
        first_stamp.source_handoff_generation
    );
    assert_eq!(
        execution.views[&view_id].admission.last_target_generation,
        first_stamp.generation
    );
    let mut foreign_namespace = namespace;
    foreign_namespace.device_epoch += 1;
    assert!(matches!(
        execution.validate_viewport_reservation(
            &successor,
            foreign_namespace.backend_instance_id,
            foreign_namespace.device_epoch,
        ),
        Err(WgpuRenderHostError::StaleViewportReservation)
    ));
    execution.retire_image_target(&producer);
    execution
        .activate_image_target(&producer, successor)
        .unwrap();
    assert!(execution.image_target_matches(&producer, source, resized_target));
}

#[test]
fn matching_runtime_and_target_reservations_publish_together_or_not_at_all() {
    #[derive(Debug)]
    struct RuntimeReservation {
        source: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    }

    #[derive(Default)]
    struct Publication {
        runtime: Option<RenderFrameSourceGeneration>,
        target: Option<ImageTargetStamp>,
    }

    impl Publication {
        fn commit(
            &mut self,
            runtime: RuntimeReservation,
            target: ImageTargetStamp,
        ) -> Result<(), (RuntimeReservation, ImageTargetStamp)> {
            if self.runtime.is_some()
                || self.target.is_some()
                || runtime.source != target.source_generation
                || runtime.target != target.target
            {
                return Err((runtime, target));
            }
            self.runtime = Some(runtime.source);
            self.target = Some(target);
            Ok(())
        }
    }

    let frame = test_image_frame();
    let source = frame.source_generation();
    let target = image_target(&frame);
    let target_reservation = next_viewport_reservation_stamp(
        WgpuRenderProducerId::new(1),
        0,
        0,
        None,
        11,
        2,
        source,
        target,
    )
    .unwrap();
    let mut publication = Publication::default();
    let mismatched_runtime = RuntimeReservation {
        source: test_image_frame().source_generation(),
        target,
    };
    let (_runtime, target_reservation) = publication
        .commit(mismatched_runtime, target_reservation)
        .expect_err("mismatched reservations must remain unpublished");
    assert!(publication.runtime.is_none());
    assert!(publication.target.is_none());

    publication
        .commit(RuntimeReservation { source, target }, target_reservation)
        .unwrap();
    assert_eq!(publication.runtime, Some(source));
    assert_eq!(publication.target, Some(target_reservation));
}

#[cfg(feature = "stock-ui")]
#[test]
fn prepared_ui_handoff_moves_original_attempts_and_refuses_bound_reinterpretation() {
    let backend_generation = UiPresentationBackendGeneration::new(21, 4).unwrap();
    let (outcomes, original) = editor_ui_submission(83);
    let WgpuUiSubmissionState::Prepared(original_submissions) = &original.state else {
        panic!("the fixture must own its original prepared attempts");
    };
    let original_storage = original_submissions.as_ptr();
    let original_identity = original_submissions[0].submission().identity();
    let original_attempt =
        nara_ui_render::__private::prepared_attempt(original_submissions[0].submission());
    let attempts = original.into_prepared_submissions().unwrap();
    assert_eq!(attempts.as_ptr(), original_storage);
    let submitted =
        nara_ui_render::advanced::bind_backend_submission_batch(attempts, backend_generation)
            .unwrap();
    let bound_storage = submitted.as_ptr();
    let bound = WgpuUiSubmission::from_state(WgpuUiSubmissionState::Submitted(submitted));
    let bound = bound.into_prepared_submissions().unwrap_err();
    assert_eq!(bound.state.bound_submissions().as_ptr(), bound_storage);
    let completed = publish_editor_ui_outcomes(outcomes, bound.presented());
    assert!(matches!(
        completed.as_slice(),
        [UiPresentationOutcome::Presented(commit)]
            if commit.attempt() == original_attempt
                && commit.submission() == original_identity
                && commit.backend_generation() == backend_generation
    ));
}

#[cfg(feature = "stock-ui")]
#[test]
fn editor_ui_completion_maps_every_terminal_path_without_stranding_permits() {
    let backend_generation = UiPresentationBackendGeneration::new(21, 4).unwrap();

    let (outcomes, mut presented) = editor_ui_submission(31);
    presented.bind_backend(backend_generation).unwrap();
    assert!(matches!(
        publish_editor_ui_outcomes(outcomes, presented.presented()).remove(0),
        UiPresentationOutcome::Presented(_)
    ));

    let (outcomes, mut deferred) = editor_ui_submission(32);
    deferred.bind_backend(backend_generation).unwrap();
    let upload_deferred = WgpuRenderHostError::Backend(WgpuRenderError::UploadDeferred {
        message: "shared upload budget exhausted".to_owned(),
    });
    assert!(matches!(
        publish_editor_ui_outcomes(outcomes, deferred.failed(&upload_deferred)).remove(0),
        UiPresentationOutcome::Deferred { .. }
    ));

    let (outcomes, rejected) = editor_ui_submission(33);
    assert!(matches!(
        publish_editor_ui_outcomes(
            outcomes,
            rejected.content_rejected("packet lowering rejected".to_owned())
        )
        .remove(0),
        UiPresentationOutcome::ContentRejected { .. }
    ));

    let (outcomes, mut target_fault) = editor_ui_submission(34);
    target_fault.bind_backend(backend_generation).unwrap();
    assert!(matches!(
        publish_editor_ui_outcomes(
            outcomes,
            target_fault.backend_fault(
                Some(backend_generation),
                UiPresentationBackendFaultDomain::Target,
                "viewport target retired".to_owned(),
            )
        )
        .remove(0),
        UiPresentationOutcome::BackendFault {
            domain: UiPresentationBackendFaultDomain::Target,
            ..
        }
    ));

    let (outcomes, mut device_fault) = editor_ui_submission(35);
    device_fault.bind_backend(backend_generation).unwrap();
    assert!(matches!(
        publish_editor_ui_outcomes(
            outcomes,
            device_fault.backend_fault(
                Some(backend_generation),
                UiPresentationBackendFaultDomain::Device,
                "device lost".to_owned(),
            )
        )
        .remove(0),
        UiPresentationOutcome::BackendFault {
            backend_generation: Some(generation),
            domain: UiPresentationBackendFaultDomain::Device,
            ..
        } if generation == backend_generation
    ));

    let (outcomes, superseded) = editor_ui_submission(36);
    let successor = nara_ui_render::__private::prepared_attempt(&test_ui_prepared_submission(37));
    assert!(matches!(
        publish_editor_ui_outcomes(
            outcomes,
            superseded.superseded(successor, "new viewport candidate".to_owned())
        )
        .remove(0),
        UiPresentationOutcome::Superseded {
            successor: actual,
            ..
        } if actual == successor
    ));
}

#[cfg(feature = "stock-ui")]
#[test]
fn editor_ui_presented_completion_requires_the_exact_final_composite() {
    let mut execution = WgpuRenderHost::offscreen();
    let Some(namespace) = test_namespace(&mut execution) else {
        return;
    };
    let frame = test_image_frame();
    let image = image_target(&frame);
    let submitted = WgpuSubmittedImageReceipt::new(
        WgpuRenderProducerId::new(1),
        image,
        8,
        frame.source_generation(),
        3,
        21,
        4,
        12,
    );
    let window = WgpuEditorWindowPresentedReceipt::new(
        WindowId::PRIMARY,
        5,
        submitted.backend_instance_id(),
        submitted.device_epoch(),
        17,
    );
    let (outcomes, mut ui) = editor_ui_submission(38);
    ui.bind_backend(UiPresentationBackendGeneration::new(21, 4).unwrap())
        .unwrap();
    let output = allocated_test_output(&execution, &namespace, submitted);
    let display = WgpuImageDisplay::issue(output.clone());
    let mirror = WgpuImageDisplay::issue(output.clone());
    let submission = WgpuUiImageSubmission {
        display: display.clone(),
        interaction: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        producer_closed: Arc::new(AtomicBool::new(false)),
        ui,
    };
    assert_eq!(submission.image(), submitted);
    assert_eq!(submission.output(), &output);
    assert_eq!(submission.display(), &display);
    let foreign = WgpuEditorCompositeReceipt::new(
        WgpuSubmittedImageReceipt::new(
            WgpuRenderProducerId::new(1),
            image,
            9,
            frame.source_generation(),
            3,
            21,
            4,
            12,
        ),
        display.occurrence(),
        window,
        1,
    );
    let submission = submission
        .presented(foreign)
        .expect_err("a foreign image receipt must not commit Runtime UI presentation");
    let submission = submission
        .presented(WgpuEditorCompositeReceipt::new(
            submitted,
            mirror.occurrence(),
            window,
            1,
        ))
        .expect_err("a passive same-window mirror must not consume the original UI permit");
    let exact = WgpuEditorCompositeReceipt::new(submitted, display.occurrence(), window, 1);

    let completed = publish_editor_ui_outcomes(outcomes, submission.presented(exact).unwrap());
    assert_eq!(completed.len(), 1);
    assert!(matches!(completed[0], UiPresentationOutcome::Presented(_)));
}

#[test]
fn editor_admission_validation_is_read_only_until_success_is_committed() {
    let source = test_image_frame().source_generation();
    let foreign = test_image_frame().source_generation();
    let mut admission = ProducerAdmission {
        source_generation: Some(source),
        ..Default::default()
    };
    admission.admit(source, 8).unwrap();
    admission.admit(source, 8).unwrap();
    assert_eq!(admission.last_frame_index, None);

    // Seed a successful-publication bound, rather than pretending validation submits work.
    admission.last_frame_index = Some(8);
    for frame_index in [7, 8] {
        assert!(matches!(
            admission.admit(source, frame_index),
            Err(WgpuRenderError::FrameAlreadyConsumed { frame_index: actual })
                if actual == frame_index
        ));
    }
    admission.admit(source, 9).unwrap();
    admission.admit(source, 9).unwrap();
    assert!(matches!(
        admission.admit(foreign, 9),
        Err(WgpuRenderError::StaleFrameGeneration { expected, actual })
            if expected == source && actual == foreign
    ));
    assert_eq!(admission.source_generation, Some(source));
    assert_eq!(admission.last_frame_index, Some(8));
}

#[cfg(feature = "stock-ui")]
#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_editor_ui_rejected_after_close_or_loss_retains_the_original_permit() {
    for lost in [false, true] {
        let mut execution = WgpuRenderHost::offscreen();
        execution.begin_frame(1).unwrap();
        let frame = test_image_frame();
        let producer = execution.create_producer().unwrap();
        let view_id = producer.id();
        let reservation = execution
            .reserve_image_target(&producer, frame.source_generation(), image_target(&frame))
            .unwrap();
        execution
            .activate_image_target(&producer, reservation)
            .unwrap();
        let namespace = execution.backend.prepare_image_device().unwrap();
        let backend_generation = UiPresentationBackendGeneration::new(
            namespace.backend_instance_id,
            namespace.device_epoch,
        )
        .unwrap();
        let (outcomes, mut ui) = editor_ui_submission_with_packet(
            if lost { 91 } else { 90 },
            compile_test_ui_packet(
                nara_ui_kernel::UiVisual::solid(nara_ui_kernel::UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
                RenderTarget::Image(image_target(&frame).image()),
                &nara_ui_render::UiPreparedImages::default(),
            ),
        );
        let (packet, attempt) = match &ui.state {
            WgpuUiSubmissionState::Prepared(submissions) => (
                Arc::clone(submissions[0].submission().packet()),
                nara_ui_render::__private::prepared_attempt(submissions[0].submission()),
            ),
            WgpuUiSubmissionState::Submitted(_) => {
                panic!("fixture must start with its original prepared permit")
            }
        };
        let input = test_stock_capture(
            frame,
            crate::WgpuStockCapture::new().with_ui(vec![Arc::clone(&packet)], None, None),
        );
        let output = execution
            .submit_image_in_namespace(view_id, &namespace, &input, Some(&mut ui))
            .unwrap();
        let original_submission = std::ptr::from_ref(&ui.state.bound_submissions()[0]);
        let original_identity = ui.state.bound_submissions()[0].submission().identity();
        let display = execution.display_image(output.clone()).unwrap();
        let interaction = Arc::clone(&execution.views[&view_id].interaction);
        let producer_closed = Arc::clone(&producer.closed);
        let submission = WgpuUiImageSubmission {
            display: display.clone(),
            interaction: Arc::clone(&interaction),
            producer_closed: Arc::clone(&producer_closed),
            ui,
        };
        let window = WgpuEditorWindowPresentedReceipt::new(
            WindowId::PRIMARY,
            1,
            namespace.backend_instance_id,
            namespace.device_epoch,
            1,
        );
        let composite =
            WgpuEditorCompositeReceipt::new(output.receipt(), display.occurrence(), window, 1);
        assert_eq!(
            read_output_pixel(&mut execution, &output, 16, 16),
            [255, 255, 255, 255]
        );
        let domain = if lost {
            namespace.device.destroy();
            namespace.device.poll(wgpu::PollType::Poll).unwrap();
            assert!(
                !output.is_current(),
                "the original device signal immediately revokes retained pixels"
            );
            assert!(interaction.load(std::sync::atomic::Ordering::Acquire));
            UiPresentationBackendFaultDomain::Device
        } else {
            drain_submission_prefix(&execution.retire_producer(&producer).unwrap());
            assert!(!interaction.load(std::sync::atomic::Ordering::Acquire));
            assert!(
                output.is_current(),
                "producer close preserves passive pixel leases"
            );
            assert_eq!(
                read_output_pixel(&mut execution, &output, 16, 16),
                [255, 255, 255, 255]
            );
            UiPresentationBackendFaultDomain::Target
        };
        let returned = submission
            .presented(composite)
            .expect_err("an exact old composite cannot restore revoked UI authority");
        assert!(Arc::ptr_eq(&returned.output().0, &output.0));
        assert!(Arc::ptr_eq(&returned.interaction, &interaction));
        assert!(Arc::ptr_eq(&returned.producer_closed, &producer_closed));
        assert_eq!(returned.display(), &display);
        assert_eq!(returned.ui.state.submission_count(), 1);
        let original = &returned.ui.state.bound_submissions()[0];
        assert!(std::ptr::eq(original, original_submission));
        assert_eq!(original.submission().identity(), original_identity);
        assert!(Arc::ptr_eq(original.submission().packet(), &packet));
        let returned = returned
            .presented(composite)
            .expect_err("repeating the composite must retain the same one-shot permit");
        let completed = publish_editor_ui_outcomes(
            outcomes,
            returned.backend_fault(
                Some(backend_generation),
                domain,
                "original display authority revoked".to_owned(),
            ),
        );
        assert_eq!(completed.len(), 1);
        assert!(
            matches!(&completed[0], UiPresentationOutcome::BackendFault {
            attempt: Some(actual), backend_generation: Some(generation), domain: actual_domain, ..
        } if *actual == attempt && *generation == backend_generation && *actual_domain == domain)
        );
        if lost {
            drain_submission_prefix(&execution.retire_producer(&producer).unwrap());
        }
    }
}

#[test]
fn source_replacement_resets_only_its_producer_clock() {
    let mut execution = WgpuRenderHost::offscreen();
    let first = test_image_frame();
    let other = test_image_frame();
    let replacement = test_image_frame().source_generation();
    let first_view = WgpuRenderProducerId::new(1);
    let other_view = WgpuRenderProducerId::new(2);
    let first_stamp = activate_test_view(
        &mut execution,
        first_view,
        first.source_generation(),
        image_target(&first),
    );
    let other_stamp = activate_test_view(
        &mut execution,
        other_view,
        other.source_generation(),
        image_target(&other),
    );
    execution
        .views
        .get_mut(&first_view)
        .unwrap()
        .admission
        .last_frame_index = Some(8);
    execution
        .views
        .get_mut(&other_view)
        .unwrap()
        .admission
        .last_frame_index = Some(13);
    let replacement_stamp = activate_test_view(
        &mut execution,
        first_view,
        replacement,
        image_target(&first),
    );

    assert_eq!(replacement_stamp.generation, first_stamp.generation + 1);
    assert_eq!(
        replacement_stamp.source_handoff_generation,
        first_stamp.source_handoff_generation + 1
    );
    let admission = &execution.views[&first_view].admission;
    assert_eq!(admission.last_frame_index, None);
    admission.admit(replacement, 1).unwrap();
    assert!(matches!(
        admission.admit(first.source_generation(), 9),
        Err(WgpuRenderError::StaleFrameGeneration { expected, actual })
            if expected == replacement && actual == first.source_generation()
    ));
    let other_admission = &execution.views[&other_view].admission;
    assert_eq!(
        other_admission.last_target_generation,
        other_stamp.generation
    );
    assert_eq!(
        other_admission.source_generation,
        Some(other.source_generation())
    );
    assert_eq!(other_admission.last_frame_index, Some(13));
    other_admission
        .admit(other.source_generation(), 14)
        .unwrap();
}

#[test]
fn viewport_generation_exhaustion_never_wraps_or_requires_a_spurious_handoff() {
    let frame = test_image_frame();
    let source = frame.source_generation();
    let target = image_target(&frame);
    let view_id = WgpuRenderProducerId::new(1);
    assert!(matches!(
        next_viewport_reservation_stamp(view_id, u64::MAX, 1, Some(source), 1, 1, source, target),
        Err(WgpuRenderHostError::TargetGenerationExhausted)
    ));
    assert!(matches!(
        next_viewport_reservation_stamp(
            view_id,
            1,
            u64::MAX,
            Some(source),
            1,
            1,
            test_image_frame().source_generation(),
            target
        ),
        Err(WgpuRenderHostError::SourceHandoffGenerationExhausted)
    ));
    let resized =
        next_viewport_reservation_stamp(view_id, 1, u64::MAX, Some(source), 1, 1, source, target)
            .unwrap();
    assert_eq!(resized.generation, 2);
    assert_eq!(resized.source_handoff_generation, u64::MAX);
}

#[test]
fn composite_generation_exhaustion_cannot_publish_or_wrap() {
    let mut execution = WgpuRenderHost::offscreen();
    let Some(namespace) = test_namespace(&mut execution) else {
        return;
    };
    let frame = test_image_frame();
    let receipt = WgpuSubmittedImageReceipt::new(
        WgpuRenderProducerId::new(1),
        image_target(&frame),
        1,
        frame.source_generation(),
        1,
        namespace.backend_instance_id,
        namespace.device_epoch,
        1,
    );
    let display = WgpuImageDisplay::issue(allocated_test_output(&execution, &namespace, receipt));
    let window = WgpuEditorWindowPresentedReceipt::new(
        WindowId::PRIMARY,
        1,
        namespace.backend_instance_id,
        namespace.device_epoch,
        1,
    );
    let sample = ImageSample {
        image: receipt,
        occurrence: display.occurrence(),
    };
    let mut generation = u64::MAX;
    assert_eq!(
        join_composite_receipt(&mut generation, Some(receipt), None, window).unwrap(),
        None
    );
    assert!(matches!(
        join_composite_receipt(&mut generation, Some(receipt), Some(sample), window),
        Err(WgpuRenderHostError::PresentationGenerationExhausted)
    ));
    assert!(matches!(
        join_composite_receipts(&mut generation, &[display], &[sample], window),
        Err(WgpuRenderHostError::PresentationGenerationExhausted)
    ));
    assert_eq!(generation, u64::MAX);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_offscreen_outputs_retain_exact_pixels_through_resize_and_close() {
    let mut execution = WgpuRenderHost::offscreen();
    execution.begin_frame(101).unwrap();
    let frame = test_image_frame();
    let source = frame.source_generation();
    let target = image_target(&frame);
    let producer = execution.create_producer().unwrap();
    let reservation = execution
        .reserve_image_target(&producer, source, target)
        .unwrap();
    let original_identity = reservation.identity();
    execution
        .activate_image_target(&producer, reservation)
        .unwrap();
    let first_input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        1,
        target,
        Color::rgba(1.0, 0.0, 0.0, 1.0),
    ));
    let first = execution.submit(&producer, &first_input).unwrap();
    let retained = first.clone();
    assert!(Arc::ptr_eq(&first.0, &retained.0));
    assert!(original_identity.matches_submission(first.receipt()));

    execution.begin_frame(102).unwrap();
    let second_input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        2,
        target,
        Color::rgba(0.0, 1.0, 0.0, 1.0),
    ));
    let second = execution.submit(&producer, &second_input).unwrap();
    assert!(!Arc::ptr_eq(&first.0, &second.0));
    assert_ne!(first.0.texture, second.0.texture);
    assert_ne!(
        first.receipt().output_version(),
        second.receipt().output_version()
    );
    assert_eq!(
        first.receipt().target_generation(),
        second.receipt().target_generation()
    );
    assert_eq!(first.receipt().render_transaction_frame_index(), 1);
    assert_eq!(second.receipt().render_transaction_frame_index(), 2);
    assert_eq!(
        read_output_pixel(&mut execution, &first, 16, 16),
        [255, 0, 0, 255]
    );
    assert_eq!(
        read_output_pixel(&mut execution, &second, 16, 16),
        [0, 255, 0, 255]
    );

    let resized = RenderImagePacket::new(target.image(), Extent2d::new(32, 32).unwrap()).unwrap();
    let reservation = execution
        .reserve_image_target(&producer, source, resized)
        .unwrap();
    execution.retire_image_target(&producer);
    execution
        .activate_image_target(&producer, reservation)
        .unwrap();
    execution.begin_frame(103).unwrap();
    let resized_input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        3,
        resized,
        Color::rgba(0.0, 0.0, 1.0, 1.0),
    ));
    let third = execution.submit(&producer, &resized_input).unwrap();
    assert_eq!(
        third.receipt().target_generation(),
        first.receipt().target_generation() + 1
    );
    assert!(!original_identity.matches_submission(third.receipt()));
    let prefix = execution
        .retire_producer(&producer)
        .expect("submitted producer returns its own prefix");
    assert!(execution.views.is_empty());
    assert!(first.is_current());
    assert!(second.is_current());
    assert_eq!(
        read_output_pixel(&mut execution, &retained, 16, 16),
        [255, 0, 0, 255]
    );
    assert_eq!(
        read_output_pixel(&mut execution, &second, 16, 16),
        [0, 255, 0, 255]
    );
    assert_eq!(
        read_output_pixel(&mut execution, &third, 16, 16),
        [0, 0, 255, 255]
    );
    execution.display_image(retained).unwrap();
    drain_submission_prefix(&prefix);

    let mut namespace = execution.backend.prepare_image_device().unwrap();
    assert!(namespace.surface.is_none());
    namespace.device_epoch += 1;
    assert!(matches!(
        execution.validate_viewport_submission(&namespace, &first),
        Err(WgpuRenderHostError::StaleImageSubmission)
    ));
    namespace.device_epoch -= 1;
    namespace.backend_instance_id += 1;
    assert!(matches!(
        execution.validate_viewport_submission(&namespace, &first),
        Err(WgpuRenderHostError::StaleImageSubmission)
    ));
    let facts = execution.frame_transaction_stats();
    assert_eq!(facts.surface_acquire_attempts(), 0);
    assert_eq!(facts.presents(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_producer_retirement_settles_while_another_producer_keeps_submitting() {
    let mut execution = WgpuRenderHost::offscreen();
    execution.begin_frame(1).unwrap();
    let first_frame = test_image_frame();
    let second_frame = test_image_frame();
    let first_producer = execution.create_producer().unwrap();
    let second_producer = execution.create_producer().unwrap();
    let first_view = first_producer.id();
    let second_view = second_producer.id();
    for (producer, frame) in [
        (&first_producer, &first_frame),
        (&second_producer, &second_frame),
    ] {
        let reservation = execution
            .reserve_image_target(producer, frame.source_generation(), image_target(frame))
            .unwrap();
        execution
            .activate_image_target(producer, reservation)
            .unwrap();
    }
    let first_input = test_stock_input(image_frame_for(
        first_frame.source_generation(),
        first_frame.views()[0].id,
        1,
        image_target(&first_frame),
        Color::rgba(1.0, 0.0, 0.0, 1.0),
    ));
    let retained = execution.submit(&first_producer, &first_input).unwrap();
    let second_input = test_stock_input(second_frame);
    let second_source = second_input.packet().source_generation();
    let second_target = image_target(second_input.packet());
    execution.submit(&second_producer, &second_input).unwrap();
    assert_eq!(execution.frame_transaction_stats().packet_admissions(), 2);
    assert_eq!(execution.frame_transaction_stats().queue_submissions(), 2);
    let old_reservation = execution
        .reserve_image_target(
            &first_producer,
            first_frame.source_generation(),
            image_target(&first_frame),
        )
        .unwrap();
    let prefix = execution.retire_producer(&first_producer).unwrap();
    assert!(!execution.views.contains_key(&first_view));
    assert!(execution.views.contains_key(&second_view));
    assert!(matches!(
        execution.reserve_image_target(
            &first_producer,
            first_frame.source_generation(),
            image_target(&first_frame)
        ),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(matches!(
        execution.submit(&first_producer, &first_input),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(matches!(
        execution.activate_image_target(&first_producer, old_reservation),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(
        !execution.views.contains_key(&first_view),
        "rejected work cannot resurrect a retired producer"
    );
    assert_eq!(execution.frame_transaction_stats().packet_admissions(), 2);
    assert_eq!(execution.frame_transaction_stats().queue_submissions(), 2);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut frame_index = 2;
    loop {
        execution.begin_frame(frame_index).unwrap();
        let input = test_stock_input(image_frame_for(
            second_source,
            second_input.packet().views()[0].id,
            frame_index,
            second_target,
            Color::BLACK,
        ));
        execution.submit(&second_producer, &input).unwrap();
        if prefix.poll().unwrap() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline && frame_index < 128,
            "producer-local completion followed the progressing producer"
        );
        frame_index += 1;
    }
    assert!(prefix.is_complete());
    assert!(
        prefix.poll().unwrap(),
        "settlement must remain stable after later work"
    );
    execution.begin_frame(frame_index + 1).unwrap();
    let later_input = test_stock_input(image_frame_for(
        second_source,
        second_input.packet().views()[0].id,
        frame_index + 1,
        second_target,
        Color::BLACK,
    ));
    execution.submit(&second_producer, &later_input).unwrap();
    assert_eq!(
        execution.views[&second_view].admission.last_frame_index,
        Some(frame_index + 1)
    );
    assert_eq!(
        read_output_pixel(&mut execution, &retained, 16, 16),
        [255, 0, 0, 255]
    );
    drain_submission_prefix(&execution.retire_producer(&second_producer).unwrap());
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_failed_submission_allows_retry_of_the_same_original_input() {
    let mut execution = WgpuRenderHost::offscreen();
    execution.begin_frame(1).unwrap();
    let frame = test_image_frame();
    let source = frame.source_generation();
    let target = image_target(&frame);
    let producer = execution.create_producer().unwrap();
    let view_id = producer.id();
    let first_input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        1,
        target,
        Color::rgba(1.0, 0.0, 0.0, 1.0),
    ));
    let reservation = execution
        .reserve_image_target(&producer, source, target)
        .unwrap();
    execution
        .activate_image_target(&producer, reservation)
        .unwrap();
    let first = execution.submit(&producer, &first_input).unwrap();
    assert_eq!(
        read_output_pixel(&mut execution, &first, 16, 16),
        [255, 0, 0, 255]
    );

    execution.begin_frame(2).unwrap();
    let input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        2,
        target,
        Color::rgba(0.0, 1.0, 0.0, 1.0),
    ));
    // The production hook destroys only this attempt's already-encoded attachment immediately
    // before the real Queue::submit. Native error scopes decide whether it may publish.
    IMAGE_SUBMISSION_FAILURE.with(|failure| failure.set(true));
    assert!(matches!(
        execution.submit(&producer, &input),
        Err(WgpuRenderHostError::Backend(
            WgpuRenderError::ContentRejected { .. }
        ))
    ));
    assert!(!IMAGE_SUBMISSION_FAILURE.with(|failure| failure.get()));
    assert_eq!(
        execution.views[&view_id].admission.last_frame_index,
        Some(1)
    );
    assert_eq!(execution.frame_transaction_stats().packet_admissions(), 0);
    assert_eq!(execution.frame_transaction_stats().queue_submissions(), 0);
    let failed_prefix = execution.views[&view_id]
        .submitted_prefix
        .clone()
        .expect("failed native attempt keeps original custody");
    assert_eq!(
        read_output_pixel(&mut execution, &first, 16, 16),
        [255, 0, 0, 255]
    );

    let output = execution.submit(&producer, &input).unwrap();
    assert_eq!(
        execution.views[&view_id].admission.last_frame_index,
        Some(2)
    );
    assert_eq!(execution.frame_transaction_stats().packet_admissions(), 1);
    assert_eq!(execution.frame_transaction_stats().queue_submissions(), 1);
    assert_ne!(output.0.texture, first.0.texture);
    assert_ne!(
        output.receipt().output_version(),
        first.receipt().output_version()
    );
    assert_eq!(
        output.receipt().target_generation(),
        first.receipt().target_generation()
    );
    assert_eq!(output.receipt().render_transaction_frame_index(), 2);
    assert!(matches!(
        execution.submit(&producer, &input),
        Err(WgpuRenderHostError::Backend(
            WgpuRenderError::FrameAlreadyConsumed { frame_index: 2 }
        ))
    ));
    assert_eq!(
        read_output_pixel(&mut execution, &output, 16, 16),
        [0, 255, 0, 255]
    );
    assert_eq!(
        read_output_pixel(&mut execution, &first, 16, 16),
        [255, 0, 0, 255]
    );
    drain_submission_prefix(&failed_prefix);
    drain_submission_prefix(&execution.retire_producer(&producer).unwrap());
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
#[cfg(feature = "stock-ui")]
fn native_image_preflight_refusal_preserves_the_original_reserved_storage_for_retry() {
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    let frame = test_image_frame();
    let source = frame.source_generation();
    let target = image_target(&frame);
    let producer = host.create_producer().unwrap();
    let reservation = host
        .reserve_image_target(&producer, source, target)
        .unwrap();
    host.activate_image_target(&producer, reservation).unwrap();
    let (texture, view, guard) = host.views[&producer.id()]
        .target
        .as_ref()
        .unwrap()
        .initial_storage
        .as_ref()
        .map(|(texture, view, guard)| (texture.clone(), view.clone(), Arc::clone(guard)))
        .unwrap();
    let bytes = host.retained_resource_bytes();
    let [image, _] = crate::readback_tests::two_image_versions();
    let input = test_stock_capture(
        frame,
        crate::WgpuStockCapture::new().with_ui(
            vec![test_ui_image_packet(
                image,
                RenderTarget::Image(target.image()),
            )],
            None,
            None,
        ),
    );
    *host.backend.stock_quad_resources().textures_mut() =
        crate::texture::WgpuSpriteTextureCache::with_image_residency_limits(1, 3);
    let error = host.submit(&producer, &input).unwrap_err();
    assert!(error.is_content_rejected());
    let preserved = host.views[&producer.id()]
        .target
        .as_ref()
        .unwrap()
        .initial_storage
        .as_ref()
        .expect("pure preflight refusal preserves the separately admitted target");
    assert_eq!(preserved.0, texture);
    assert_eq!(preserved.1, view);
    assert!(Arc::ptr_eq(&preserved.2, &guard));
    assert_eq!(host.retained_resource_bytes(), bytes);
    assert_eq!(host.backend.texture_stats().uploaded_images, 0);
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);
    *host.backend.stock_quad_resources().textures_mut() =
        crate::texture::WgpuSpriteTextureCache::with_image_residency_limits(1, 4);
    let output = host.submit(&producer, &input).unwrap();
    assert_eq!(output.0.texture, texture);
    assert!(
        host.views[&producer.id()]
            .target
            .as_ref()
            .unwrap()
            .initial_storage
            .is_none()
    );
    assert_eq!(
        read_output_pixel(&mut host, &output, 16, 16),
        [255, 0, 0, 255]
    );
    drain_submission_prefix(&host.retire_producer(&producer).unwrap());
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_output_budget_counts_exact_storage_until_callbacks_and_last_leases_drop() {
    const BYTES: u64 = 64 * 64 * 4;
    let mut execution = WgpuRenderHost::offscreen().with_resource_byte_limit(2 * BYTES - 1);
    execution.begin_frame(1).unwrap();
    let frame = test_image_frame();
    let source = frame.source_generation();
    let target = image_target(&frame);
    let producer = execution.create_producer().unwrap();
    let view_id = producer.id();
    let reservation = execution
        .reserve_image_target(&producer, source, target)
        .unwrap();
    assert_eq!(
        execution.retained_resource_bytes(),
        BYTES,
        "configuration preadmits its allocation"
    );
    execution
        .activate_image_target(&producer, reservation)
        .unwrap();
    let first_input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        1,
        target,
        Color::rgba(1.0, 0.0, 0.0, 1.0),
    ));
    let first = execution.submit(&producer, &first_input).unwrap();
    assert_eq!(
        execution.retained_resource_bytes(),
        BYTES,
        "first publication consumes the preadmitted storage"
    );
    drain_submission_prefix(execution.views[&view_id].submitted_prefix.as_ref().unwrap());
    assert_eq!(
        execution.retained_resource_bytes(),
        BYTES,
        "a completed output remains leased"
    );

    execution.begin_frame(2).unwrap();
    let second_input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        2,
        target,
        Color::rgba(0.0, 1.0, 0.0, 1.0),
    ));
    assert!(matches!(
        execution.submit(&producer, &second_input),
        Err(WgpuRenderHostError::Program(
            crate::WgpuRenderProgramError::ResourceCapacity
        ))
    ));
    assert_eq!(execution.retained_resource_bytes(), BYTES);
    assert_eq!(
        execution.views[&view_id].admission.last_frame_index,
        Some(1)
    );
    assert_eq!(
        read_output_pixel(&mut execution, &first, 16, 16),
        [255, 0, 0, 255]
    );

    execution = execution.with_resource_byte_limit(2 * BYTES);
    let second = execution.submit(&producer, &second_input).unwrap();
    assert_eq!(execution.retained_resource_bytes(), 2 * BYTES);
    drain_submission_prefix(execution.views[&view_id].submitted_prefix.as_ref().unwrap());
    assert_eq!(
        read_output_pixel(&mut execution, &second, 16, 16),
        [0, 255, 0, 255]
    );
    assert_eq!(
        execution.retained_resource_bytes(),
        2 * BYTES,
        "callbacks cannot revoke retained outputs"
    );
    let last_lease = first.clone();
    drop(first);
    assert_eq!(execution.retained_resource_bytes(), 2 * BYTES);
    drop(last_lease);
    assert_eq!(execution.retained_resource_bytes(), BYTES);
    drop(second);
    assert_eq!(execution.retained_resource_bytes(), 0);

    execution = execution.with_resource_byte_limit(BYTES - 1);
    execution.begin_frame(3).unwrap();
    let third_input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        3,
        target,
        Color::BLACK,
    ));
    assert!(matches!(
        execution.submit(&producer, &third_input),
        Err(WgpuRenderHostError::Program(
            crate::WgpuRenderProgramError::ResourceCapacity
        ))
    ));
    assert_eq!(
        execution.views[&view_id].admission.last_frame_index,
        Some(2)
    );
    execution = execution.with_resource_byte_limit(BYTES);
    let third = execution.submit(&producer, &third_input).unwrap();
    assert_eq!(execution.retained_resource_bytes(), BYTES);
    drop(third);
    drain_submission_prefix(&execution.retire_producer(&producer).unwrap());
    assert_eq!(
        execution.retained_resource_bytes(),
        0,
        "submitted use releases its charge at actual callback settlement"
    );
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_allocation_rejection_preserves_last_good_and_allows_same_capture_retry() {
    let mut execution = WgpuRenderHost::offscreen();
    execution.begin_frame(1).unwrap();
    let frame = test_image_frame();
    let source = frame.source_generation();
    let target = image_target(&frame);
    let producer = execution.create_producer().unwrap();
    let view_id = producer.id();
    let reservation = execution
        .reserve_image_target(&producer, source, target)
        .unwrap();
    execution
        .activate_image_target(&producer, reservation)
        .unwrap();
    let first_input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        1,
        target,
        Color::rgba(1.0, 0.0, 0.0, 1.0),
    ));
    let first = execution.submit(&producer, &first_input).unwrap();
    execution.begin_frame(2).unwrap();
    let input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        2,
        target,
        Color::rgba(0.0, 1.0, 0.0, 1.0),
    ));
    let retained_bytes = execution.retained_resource_bytes();
    let failure = crate::allocation::AllocationFailureGuard::at("nara_program_texture_view");
    let error = execution.submit(&producer, &input).unwrap_err();
    assert!(
        failure.was_consumed(),
        "failure must exercise the actual program allocation path"
    );
    drop(failure);
    assert!(error.is_deferred());
    assert!(!error.is_content_rejected());
    assert_eq!(execution.retained_resource_bytes(), retained_bytes);
    assert_eq!(
        execution.views[&view_id].admission.last_frame_index,
        Some(1)
    );
    assert_eq!(execution.frame_transaction_stats().packet_admissions(), 0);
    assert_eq!(execution.frame_transaction_stats().queue_submissions(), 0);
    assert_eq!(
        read_output_pixel(&mut execution, &first, 16, 16),
        [255, 0, 0, 255]
    );
    let retry = execution.submit(&producer, &input).unwrap();
    assert_eq!(
        execution.views[&view_id].admission.last_frame_index,
        Some(2)
    );
    assert_eq!(
        read_output_pixel(&mut execution, &retry, 16, 16),
        [0, 255, 0, 255]
    );
    assert_eq!(
        read_output_pixel(&mut execution, &first, 16, 16),
        [255, 0, 0, 255]
    );
    drain_submission_prefix(&execution.retire_producer(&producer).unwrap());
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_loss_rejects_old_outputs_and_new_host_replays_the_original_input() {
    let frame = test_image_frame();
    let source = frame.source_generation();
    let target = image_target(&frame);
    #[cfg(not(feature = "stock-ui"))]
    let input = test_stock_input(image_frame_for(
        source,
        frame.views()[0].id,
        1,
        target,
        Color::rgba(1.0, 0.0, 0.0, 1.0),
    ));
    #[cfg(feature = "stock-ui")]
    let (input, current_input) = {
        // This fixture publishes green over red in the same Assets slot and drops the
        // original stores. Recovery must use the pinned red capture, never recapture green.
        let [red, green] = crate::readback_tests::two_image_versions();
        let logical_target = RenderTarget::Image(target.image());
        let pinned = test_ui_image_packet(red, logical_target);
        let current = test_ui_image_packet(green, logical_target);
        (
            test_stock_capture(
                image_frame_for(source, frame.views()[0].id, 1, target, Color::BLACK),
                crate::WgpuStockCapture::new().with_ui(vec![pinned], None, None),
            ),
            test_stock_capture(
                image_frame_for(source, frame.views()[0].id, 1, target, Color::BLACK),
                crate::WgpuStockCapture::new().with_ui(vec![current], None, None),
            ),
        )
    };
    let mut original = WgpuRenderHost::offscreen();
    original.begin_frame(1).unwrap();
    let original_producer = original.create_producer().unwrap();
    let reservation = original
        .reserve_image_target(&original_producer, source, target)
        .unwrap();
    original
        .activate_image_target(&original_producer, reservation)
        .unwrap();
    let old_output = original.submit(&original_producer, &input).unwrap();
    assert_eq!(
        read_output_pixel(&mut original, &old_output, 16, 16),
        [255, 0, 0, 255]
    );
    let device = original
        .with_image(&old_output, |device, _, _| device.clone())
        .unwrap();
    device.destroy();
    device.poll(wgpu::PollType::Poll).unwrap();
    assert!(!old_output.is_current());
    assert!(matches!(
        original.display_image(old_output.clone()),
        Err(WgpuRenderHostError::Backend(WgpuRenderError::DeviceLost))
    ));
    assert!(matches!(
        original.with_image(&old_output, |_, _, _| ()),
        Err(WgpuRenderHostError::Backend(WgpuRenderError::DeviceLost))
    ));
    assert!(matches!(
        original.submit(&original_producer, &input),
        Err(WgpuRenderHostError::Backend(WgpuRenderError::DeviceLost))
    ));
    assert!(matches!(
        original.prepare_policy(&original_producer, stock_policy().definition()),
        Err(WgpuRenderHostError::Backend(WgpuRenderError::DeviceLost))
    ));
    drain_submission_prefix(&original.retire_producer(&original_producer).unwrap());

    let mut replacement = WgpuRenderHost::offscreen();
    replacement.begin_frame(1).unwrap();
    assert_ne!(
        replacement.backend_instance_id(),
        original.backend_instance_id()
    );
    assert!(matches!(
        replacement.display_image(old_output.clone()),
        Err(WgpuRenderHostError::StaleImageSubmission)
    ));
    assert!(matches!(
        replacement.with_image(&old_output, |_, _, _| ()),
        Err(WgpuRenderHostError::StaleImageSubmission)
    ));
    let replacement_producer = replacement.create_producer().unwrap();
    assert_ne!(replacement_producer.id(), original_producer.id());
    let reservation = replacement
        .reserve_image_target(&replacement_producer, source, target)
        .unwrap();
    replacement
        .activate_image_target(&replacement_producer, reservation)
        .unwrap();
    // The same owned CPU input is borrowed again, including its original content snapshots.
    let recovered = replacement.submit(&replacement_producer, &input).unwrap();
    assert_eq!(
        recovered.receipt().source_generation(),
        old_output.receipt().source_generation()
    );
    assert_eq!(
        recovered.receipt().render_transaction_frame_index(),
        old_output.receipt().render_transaction_frame_index()
    );
    assert_ne!(
        recovered.receipt().output_version(),
        old_output.receipt().output_version()
    );
    assert_eq!(
        read_output_pixel(&mut replacement, &recovered, 16, 16),
        [255, 0, 0, 255]
    );
    #[cfg(feature = "stock-ui")]
    {
        let current_producer = replacement.create_producer().unwrap();
        let reservation = replacement
            .reserve_image_target(&current_producer, source, target)
            .unwrap();
        replacement
            .activate_image_target(&current_producer, reservation)
            .unwrap();
        let current = replacement
            .submit(&current_producer, &current_input)
            .unwrap();
        assert_eq!(
            read_output_pixel(&mut replacement, &current, 16, 16),
            [0, 255, 0, 255]
        );
        assert_eq!(
            read_output_pixel(&mut replacement, &recovered, 16, 16),
            [255, 0, 0, 255]
        );
        drain_submission_prefix(&replacement.retire_producer(&current_producer).unwrap());
    }
    drain_submission_prefix(&replacement.retire_producer(&replacement_producer).unwrap());
}

fn read_output_pixel(
    execution: &mut WgpuRenderHost,
    output: &WgpuImageOutput,
    x: u32,
    y: u32,
) -> [u8; 4] {
    execution
        .with_image(output, |device, queue, _| {
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("editor_output_readback"),
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &output.0.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([encoder.finish()]);
            let (sender, receiver) = std::sync::mpsc::channel();
            readback.map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_secs(10)),
                })
                .unwrap();
            receiver
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap()
                .unwrap();
            let pixel = {
                let bytes = readback.get_mapped_range(..).unwrap();
                bytes[..4].try_into().unwrap()
            };
            readback.unmap();
            pixel
        })
        .unwrap()
}

fn drain_submission_prefix(prefix: &WgpuSubmissionPrefix) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !prefix.poll().unwrap() {
        assert!(
            std::time::Instant::now() < deadline,
            "the original submission prefix did not settle"
        );
        std::thread::yield_now();
    }
    assert!(
        prefix.poll().unwrap(),
        "prefix settlement must be monotonic"
    );
}

fn test_namespace(execution: &mut WgpuRenderHost) -> Option<WgpuDeviceNamespace> {
    match execution.backend.prepare_image_device() {
        Ok(namespace) => {
            assert!(
                namespace.surface.is_none(),
                "image setup must not create a surface"
            );
            Some(namespace)
        }
        Err(error @ WgpuRenderError::AdapterUnavailable { .. })
            if std::env::var("DEAR_IMGUI_REQUIRE_WGPU_ADAPTER").as_deref() != Ok("1") =>
        {
            eprintln!("skipping image fixture without a WGPU adapter: {error}");
            None
        }
        Err(error) => panic!("the required WGPU image device is unavailable: {error}"),
    }
}

fn allocated_test_output(
    execution: &WgpuRenderHost,
    namespace: &WgpuDeviceNamespace,
    receipt: WgpuSubmittedImageReceipt,
) -> WgpuImageOutput {
    let (texture, view, reservation) = execution
        .allocate_output_storage(namespace, receipt.target())
        .unwrap();
    WgpuImageOutput(Arc::new(WgpuImageStorage {
        epoch_lost: Arc::clone(&namespace.epoch_lost),
        receipt,
        texture,
        view,
        _reservation: Arc::new(reservation),
    }))
}

fn activate_test_view(
    execution: &mut WgpuRenderHost,
    view_id: WgpuRenderProducerId,
    source: RenderFrameSourceGeneration,
    target: RenderImagePacket,
) -> ImageTargetStamp {
    let stamp = execution
        .next_viewport_reservation_stamp_for_namespace(
            view_id,
            execution.backend.instance_id(),
            execution.backend.device_epoch(),
            source,
            target,
        )
        .unwrap();
    execution
        .views
        .entry(view_id)
        .or_default()
        .admission
        .activate(stamp);
    execution.views.get_mut(&view_id).unwrap().interaction =
        Arc::new(std::sync::atomic::AtomicBool::new(true));
    stamp
}

fn test_image_frame() -> RenderFramePacket {
    let generation = RenderFrameSourceGeneration::try_acquire().unwrap();
    let image = RenderImage2d::allocate_handle().unwrap();
    let target = RenderImagePacket::new(image, Extent2d::new(64, 64).unwrap()).unwrap();
    image_frame_for(
        generation,
        nara_render::RenderViewId::try_acquire().unwrap(),
        1,
        target,
        Color::BLACK,
    )
}

fn image_frame_for(
    generation: RenderFrameSourceGeneration,
    view_id: nara_render::RenderViewId,
    frame_index: u64,
    target: RenderImagePacket,
    color: Color,
) -> RenderFramePacket {
    let viewport = ViewportRect::from_extent(target.extent()).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: view_id,
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Image(target.image()),
        viewport,
        mapping: nara_render::ViewMapping2d::orthographic(
            nara_core::Mat3::IDENTITY,
            target.extent().height as f32,
            viewport,
        )
        .unwrap(),
        order: 0,
        load: nara_render::ViewLoadOp::Clear(color),
    });
    build_image_render_frame_packet(generation, frame_index, &views, target)
        .unwrap()
        .unwrap()
}

fn image_target(packet: &RenderFramePacket) -> RenderImagePacket {
    match packet.target() {
        RenderFrameTarget::Image(image) => image,
        RenderFrameTarget::Window(_) => panic!("test packet must target an image"),
    }
}

#[cfg(feature = "stock-ui")]
fn editor_ui_submission(
    admission_id: u64,
) -> (
    nara_ui_render::__private::UiPresentationOutcomes,
    WgpuUiSubmission,
) {
    editor_ui_submission_with_packet(admission_id, test_ui_packet())
}

#[cfg(feature = "stock-ui")]
fn editor_ui_submission_with_packet(
    admission_id: u64,
    packet: Arc<UiPaintPacket>,
) -> (
    nara_ui_render::__private::UiPresentationOutcomes,
    WgpuUiSubmission,
) {
    let prepared = nara_ui_render::__private::prepare_backend_submission(
        UiPresentationAttemptId::new(admission_id).unwrap(),
        packet,
        UiPresentationEpoch::new(admission_id + 100).unwrap(),
        UiPresentationRuntimeGeneration::new(7).unwrap(),
        UiPresentationCapabilityGeneration::new(9).unwrap(),
    );
    let mut outcomes = nara_ui_render::__private::UiPresentationOutcomes::default();
    let permit = outcomes
        .reserve_backend(std::slice::from_ref(&prepared))
        .unwrap()
        .remove(0);
    let submission = UiBackendSubmission::pair(prepared, permit).unwrap();
    (
        outcomes,
        WgpuUiSubmission {
            state: WgpuUiSubmissionState::Prepared(vec![submission]),
        },
    )
}

#[cfg(feature = "stock-ui")]
fn test_ui_prepared_submission(
    admission_id: u64,
) -> nara_ui_render::__private::UiPreparedBackendSubmission {
    nara_ui_render::__private::prepare_backend_submission(
        UiPresentationAttemptId::new(admission_id).unwrap(),
        test_ui_packet(),
        UiPresentationEpoch::new(admission_id + 100).unwrap(),
        UiPresentationRuntimeGeneration::new(7).unwrap(),
        UiPresentationCapabilityGeneration::new(9).unwrap(),
    )
}

#[cfg(feature = "stock-ui")]
fn publish_editor_ui_outcomes(
    mut outcomes: nara_ui_render::__private::UiPresentationOutcomes,
    completed: Vec<UiCompletedBackendOutcome>,
) -> Vec<UiPresentationOutcome> {
    for outcome in completed {
        outcomes.commit_backend(outcome);
    }
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap()
}

#[cfg(feature = "stock-ui")]
fn test_ui_packet() -> std::sync::Arc<UiPaintPacket> {
    compile_test_ui_packet(
        nara_ui_kernel::UiVisual::solid(nara_ui_kernel::UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        RenderTarget::Window(WindowId::PRIMARY),
        &nara_ui_render::UiPreparedImages::default(),
    )
}

#[cfg(feature = "stock-ui")]
pub(super) fn test_ui_image_packet(
    snapshot: nara_image::PreparedImageSnapshot,
    target: RenderTarget,
) -> Arc<UiPaintPacket> {
    let image = nara_ui_kernel::UiImageId::new("project://editor-execution/captured.png").unwrap();
    let mut prepared = nara_ui_render::UiPreparedImages::default();
    prepared.insert(image.clone(), snapshot);
    compile_test_ui_packet(
        nara_ui_kernel::UiVisual::image(image, nara_ui_kernel::UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
        target,
        &prepared,
    )
}

#[cfg(feature = "stock-ui")]
fn compile_test_ui_packet(
    visual: nara_ui_kernel::UiVisual,
    target: RenderTarget,
    prepared: &nara_ui_render::UiPreparedImages,
) -> Arc<UiPaintPacket> {
    use nara_text::bundled_font_face;
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiLayer, UiLocalKey, UiNode, UiRoot,
        UiRootKey, UiStyle, UiSurfaceId, UiTheme,
    };

    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("editor-game-view-ui", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(
                UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0))
                    .with_key(UiLocalKey::runtime("editor-game-view-ui", "panel").unwrap())
                    .with_visual(visual),
            ),
        )
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 64.0, 64.0, 1.0, 1, 1, 1, 1).unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    nara_ui_render::UiPresentationPlanner::default()
        .plan(frame, target, prepared)
        .unwrap()
}
