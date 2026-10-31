#![cfg(not(target_arch = "wasm32"))]

use std::{
    sync::mpsc::{self, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use nara_core::{Color, Mat3};
use nara_ecs::Entity;
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
    RenderImagePacket, RenderTarget, ViewLoadOp, ViewMapping2d, ViewportRect,
    build_image_render_frame_packet,
};
use nara_render_wgpu::{
    WgpuImageInput, WgpuImageOutput, WgpuImageTargetIdentity, WgpuRenderHost, WgpuRenderHostError,
    WgpuRenderProducer, WgpuSubmissionPrefix,
};

const TIMEOUT: Duration = Duration::from_secs(10);
const RED: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);
const GREEN: Color = Color::rgba(0.0, 1.0, 0.0, 1.0);
const BLUE: Color = Color::rgba(0.0, 0.0, 1.0, 1.0);
const RED_BYTES: [u8; 4] = [255, 0, 0, 255];
const GREEN_BYTES: [u8; 4] = [0, 255, 0, 255];
const BLUE_BYTES: [u8; 4] = [0, 0, 255, 255];
const WHITE_BYTES: [u8; 4] = [255; 4];

#[test]
#[ignore = "requires an explicitly selected native GPU; adapter failure fails the prerequisite"]
fn retained_pixels_survive_resize_and_a_close_while_b_keeps_submitting() {
    let mut host = WgpuRenderHost::offscreen();
    begin(&mut host, 1);
    let a = host
        .create_producer()
        .expect("the Host must admit producer A");
    let b = host
        .create_producer()
        .expect("the Host must admit producer B");
    let a_source = source();
    let b_source = source();
    assert_ne!(a.id(), b.id());
    assert_ne!(a_source, b_source);
    assert!(a_source.runtime_generation().is_none());
    assert!(b_source.runtime_generation().is_none());

    let a_target = image_target(16, 16);
    let b_target = image_target(16, 16);
    let a_configuration = admit(&mut host, &a, a_source, a_target);
    admit(&mut host, &b, b_source, b_target);
    let a1_input = input(a_source, 1, a_target, RED);
    let a1 = host
        .submit(&a, &a1_input)
        .expect("A1 must publish red pixels");
    assert!(a_configuration.matches_submission(a1.receipt()));

    let first_display = host
        .display_image(a1.clone())
        .expect("the exact A1 output must be displayable");
    let mirror = host
        .display_image(a1.clone())
        .expect("a passive A1 mirror must have its own occurrence");
    assert_eq!(first_display.output(), mirror.output());
    assert_ne!(first_display.occurrence(), mirror.occurrence());
    assert_eq!(
        first_display.clone().occurrence(),
        first_display.occurrence()
    );

    begin(&mut host, 2);
    let a2_input = input(a_source, 2, a_target, GREEN);
    let a2 = host
        .submit(&a, &a2_input)
        .expect("A2 must publish green pixels");
    assert!(a_configuration.matches_submission(a2.receipt()));
    assert_eq!(
        a1.receipt().target_generation(),
        a2.receipt().target_generation(),
    );
    assert_ne!(a1.receipt().output_version(), a2.receipt().output_version());
    assert_eq!(a1.receipt().render_transaction_frame_index(), 1);
    assert_eq!(a2.receipt().render_transaction_frame_index(), 2);
    assert_eq!(a1.receipt().source_generation(), a_source);
    assert_eq!(a2.receipt().source_generation(), a_source);
    assert_eq!(
        first_display.output(),
        &a1,
        "display selection cannot follow A2"
    );

    // Configuration changes keep the logical image handle and the producer, not old storage.
    let resized = RenderImagePacket::new(
        a_target.image(),
        Extent2d::new(8, 24).expect("the resized image is nonempty"),
    )
    .expect("the public image descriptor must admit a nonempty extent");
    let reservation = host
        .reserve_image_target(&a, a_source, resized)
        .expect("resize must prepare an independent configuration");
    host.retire_image_target(&a);
    host.activate_image_target(&a, reservation)
        .expect("the prepared replacement must activate");
    begin(&mut host, 3);
    let a3_input = input(a_source, 3, resized, BLUE);
    let a3 = host.submit(&a, &a3_input).expect("resized A3 must publish");
    assert_eq!(a3.extent(), resized.extent());
    assert!(!a_configuration.matches_submission(a3.receipt()));
    assert!(a3.receipt().target_generation() > a2.receipt().target_generation());
    assert_ne!(a3.receipt().output_version(), a2.receipt().output_version());

    // No readback or blocking queue drain precedes the producer-local retirement probe.
    let first_b = host
        .submit(&b, &input(b_source, 1, b_target, Color::BLACK))
        .expect("B must independently accept its first capture");
    assert_eq!(first_b.receipt().render_transaction_frame_index(), 1);
    drop(first_b);
    let a_prefix = host
        .retire_producer(&a)
        .expect("submitted A must return its frozen native prefix");
    assert!(matches!(
        host.submit(&a, &a3_input),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(matches!(
        host.reserve_image_target(&a, a_source, a_target),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(
        host.retire_producer(&a).is_none(),
        "closed A cannot issue another prefix"
    );

    let deadline = Instant::now() + TIMEOUT;
    let mut b_frame = 1;
    let latest_b = loop {
        assert!(
            Instant::now() < deadline && b_frame < 257,
            "A's frozen prefix did not settle within 256 B captures and its deadline",
        );
        b_frame += 1;
        begin(&mut host, b_frame + 2);
        let output = host
            .submit(&b, &input(b_source, b_frame, b_target, Color::WHITE))
            .expect("B must keep publishing while A retires");
        assert_eq!(output.receipt().render_transaction_frame_index(), b_frame);
        assert_window_free(&host);
        if a_prefix
            .poll()
            .expect("A's original prefix must remain pollable")
        {
            break output;
        }
        thread::yield_now();
    };
    assert!(a_prefix.is_complete());
    assert!(b_frame >= 2, "B must submit after A retirement starts");
    assert_pixels(&mut host, &latest_b, WHITE_BYTES);

    // Read every pixel only after A retired. No latest-target lookup can recover these outputs.
    assert_pixels(&mut host, &a1, RED_BYTES);
    assert_pixels(&mut host, &a2, GREEN_BYTES);
    assert_pixels(&mut host, &a3, BLUE_BYTES);
    assert_pixels(&mut host, first_display.output(), RED_BYTES);
    let after_close = host
        .display_image(a1.clone())
        .expect("a retained A1 lease remains passively displayable after A close");
    assert_eq!(after_close.output(), &a1);
    assert_ne!(after_close.occurrence(), first_display.occurrence());

    b_frame += 1;
    begin(&mut host, b_frame + 2);
    let later_b = host
        .submit(&b, &input(b_source, b_frame, b_target, GREEN))
        .expect("B must remain live after A physically completes");
    assert!(a_prefix.poll().expect("A settlement must remain monotonic"));
    assert_pixels(&mut host, &later_b, GREEN_BYTES);
    assert_window_free(&host);
    settle(
        &host
            .retire_producer(&b)
            .expect("B must return its own prefix"),
    );
    drop((
        a1,
        a2,
        a3,
        first_display,
        mirror,
        after_close,
        latest_b,
        later_b,
    ));
    assert_eq!(host.retained_output_bytes(), 0);
    finish(&mut host);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; adapter failure fails the prerequisite"]
fn exact_output_budget_preserves_leases_and_retries_the_same_refused_capture() {
    const BYTES: u64 = 16 * 16 * 4;
    let mut host = WgpuRenderHost::offscreen().with_output_byte_limit(BYTES - 1);
    begin(&mut host, 1);
    let a = host
        .create_producer()
        .expect("the Host must admit producer A");
    let source = source();
    let target = image_target(16, 16);
    assert!(matches!(
        host.reserve_image_target(&a, source, target),
        Err(WgpuRenderHostError::OutputCapacity)
    ));
    assert_eq!(host.retained_output_bytes(), 0);

    host = host.with_output_byte_limit(2 * BYTES - 1);
    let reservation = host
        .reserve_image_target(&a, source, target)
        .expect("one output must fit the configuration reservation");
    assert_eq!(host.retained_output_bytes(), BYTES);
    host.activate_image_target(&a, reservation)
        .expect("configuration activation must consume its admitted allocation");
    let red_input = input(source, 1, target, RED);
    let red = host
        .submit(&a, &red_input)
        .expect("the first output must fit");
    let retained_red = host
        .display_image(red.clone())
        .expect("a display must retain the red version");
    assert_eq!(host.retained_output_bytes(), BYTES);

    begin(&mut host, 2);
    let green_input = input(source, 2, target, GREEN);
    assert!(matches!(
        host.submit(&a, &green_input),
        Err(WgpuRenderHostError::OutputCapacity)
    ));
    assert_eq!(host.retained_output_bytes(), BYTES);
    assert_pixels(&mut host, retained_red.output(), RED_BYTES);

    // Retry the same input object/index: a failed attempt must not consume publication admission.
    host = host.with_output_byte_limit(2 * BYTES);
    let green = host
        .submit(&a, &green_input)
        .expect("the same refused capture must publish at the exact two-output budget");
    assert_eq!(green.receipt().render_transaction_frame_index(), 2);
    assert_ne!(
        red.receipt().output_version(),
        green.receipt().output_version()
    );
    assert_eq!(host.retained_output_bytes(), 2 * BYTES);
    assert_pixels(&mut host, &green, GREEN_BYTES);
    assert_pixels(&mut host, retained_red.output(), RED_BYTES);
    assert_window_free(&host);

    // Settle original submission callbacks before checking last-lease accounting.
    settle(
        &host
            .retire_producer(&a)
            .expect("A must return its submitted prefix"),
    );
    assert_eq!(host.retained_output_bytes(), 2 * BYTES);
    drop(red);
    assert_eq!(host.retained_output_bytes(), 2 * BYTES);
    drop(retained_red);
    assert_eq!(host.retained_output_bytes(), BYTES);
    drop(green);
    assert_eq!(host.retained_output_bytes(), 0);
    finish(&mut host);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; adapter failure fails the prerequisite"]
fn original_cpu_input_reconstructs_after_the_original_host_has_retired() {
    let source = source();
    let target = image_target(16, 16);
    let original_input = input(source, 1, target, RED);
    let mut original_host = WgpuRenderHost::offscreen();
    begin(&mut original_host, 1);
    let original_producer = original_host
        .create_producer()
        .expect("the original Host must admit its producer");
    let original_producer_id = original_producer.id();
    admit(&mut original_host, &original_producer, source, target);
    let old_output = original_host
        .submit(&original_producer, &original_input)
        .expect("the original input must publish");
    assert_pixels(&mut original_host, &old_output, RED_BYTES);
    assert_window_free(&original_host);
    settle(
        &original_host
            .retire_producer(&original_producer)
            .expect("the original producer must have a prefix"),
    );
    finish(&mut original_host);
    drop(original_producer);
    drop(original_host);

    // This is normal Host reconstruction, not an injected device-loss event.
    let mut replacement = WgpuRenderHost::offscreen();
    begin(&mut replacement, 1);
    assert!(matches!(
        replacement.display_image(old_output.clone()),
        Err(WgpuRenderHostError::StaleImageSubmission)
    ));
    assert!(matches!(
        replacement.with_image(&old_output, |_, _, _| {
            panic!("a foreign Host output reached native readback")
        }),
        Err(WgpuRenderHostError::StaleImageSubmission)
    ));

    let replacement_producer = replacement
        .create_producer()
        .expect("the replacement Host must admit its own producer");
    assert_ne!(replacement_producer.id(), original_producer_id);
    admit(&mut replacement, &replacement_producer, source, target);
    let reconstructed = replacement
        .submit(&replacement_producer, &original_input)
        .expect("the same original CPU input must realize under the replacement Host");
    assert_eq!(reconstructed.receipt().source_generation(), source);
    assert_eq!(reconstructed.receipt().render_transaction_frame_index(), 1);
    assert_eq!(reconstructed.receipt().target(), target);
    assert_ne!(
        reconstructed.receipt().backend_instance_id(),
        old_output.receipt().backend_instance_id(),
    );
    assert_ne!(
        reconstructed.receipt().output_version(),
        old_output.receipt().output_version(),
    );
    assert_pixels(&mut replacement, &reconstructed, RED_BYTES);
    assert_window_free(&replacement);
    settle(
        &replacement
            .retire_producer(&replacement_producer)
            .expect("the replacement producer must have its own prefix"),
    );
    drop((old_output, reconstructed));
    assert_eq!(replacement.retained_output_bytes(), 0);
    finish(&mut replacement);
}

fn source() -> RenderFrameSourceGeneration {
    RenderFrameSourceGeneration::try_acquire()
        .expect("the public independent source identity space is available")
}

fn image_target(width: u32, height: u32) -> RenderImagePacket {
    RenderImagePacket::new(
        RenderImage2d::allocate_handle().expect("allocate a public logical image identity"),
        Extent2d::new(width, height).expect("the fixture uses nonempty extents"),
    )
    .expect("a logical image handle and nonempty extent form a valid target")
}

fn input(
    source: RenderFrameSourceGeneration,
    frame: u64,
    target: RenderImagePacket,
    color: Color,
) -> WgpuImageInput {
    let viewport = ViewportRect::from_extent(target.extent()).expect("nonempty image viewport");
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Image(target.image()),
        viewport,
        mapping: ViewMapping2d::orthographic(
            Mat3::IDENTITY,
            target.extent().height as f32,
            viewport,
        )
        .expect("the identity camera has a valid orthographic mapping"),
        order: 0,
        load: ViewLoadOp::Clear(color),
    });
    WgpuImageInput::new(
        build_image_render_frame_packet(source, frame, &views, target)
            .expect("the public image capture must validate")
            .expect("one extracted image view produces a packet"),
    )
}

fn begin(host: &mut WgpuRenderHost, frame: u64) {
    host.begin_frame(frame)
        .expect("native GPU/Ready acquisition is a required prerequisite, never a skipped success");
}

fn admit(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    source: RenderFrameSourceGeneration,
    target: RenderImagePacket,
) -> WgpuImageTargetIdentity {
    let reservation = host
        .reserve_image_target(producer, source, target)
        .expect("public configuration preparation must admit the image allocation");
    let identity = reservation.identity();
    host.activate_image_target(producer, reservation)
        .expect("a matching prepared image configuration must activate");
    identity
}

fn assert_window_free(host: &WgpuRenderHost) {
    let facts = host.frame_transaction_stats();
    assert_eq!(facts.surface_acquire_attempts(), 0);
    assert_eq!(facts.presents(), 0);
}

fn settle(prefix: &WgpuSubmissionPrefix) {
    let deadline = Instant::now() + TIMEOUT;
    while !prefix
        .poll()
        .expect("the original native prefix must remain pollable")
    {
        assert!(
            Instant::now() < deadline,
            "native submission prefix timed out"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(prefix.is_complete());
    assert!(
        prefix
            .poll()
            .expect("prefix completion must remain monotonic")
    );
}

fn finish(host: &mut WgpuRenderHost) {
    let deadline = Instant::now() + TIMEOUT;
    while !host
        .poll_retirement()
        .expect("final Host retirement must remain pollable")
    {
        assert!(Instant::now() < deadline, "final Host retirement timed out");
        thread::sleep(Duration::from_millis(1));
    }
}

fn assert_pixels(host: &mut WgpuRenderHost, output: &WgpuImageOutput, expected: [u8; 4]) {
    let extent = output.extent();
    assert_eq!(output.format(), wgpu::TextureFormat::Rgba8Unorm);
    let row_bytes = extent.width.checked_mul(4).expect("bounded RGBA8 row");
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_row = row_bytes.div_ceil(alignment) * alignment;
    let bytes = u64::from(padded_row) * u64::from(extent.height);
    assert!(
        bytes > 0 && bytes <= 64 * 1024,
        "readback is finite and bounded"
    );

    let (readback, prefix) = host
        .submit_image_use(
            std::slice::from_ref(output),
            |device, _queue, encoder, views| {
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("public-output-consumer-readback"),
                    size: bytes,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    views[0].texture().as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &readback,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(padded_row),
                            rows_per_image: Some(extent.height),
                        },
                    },
                    wgpu::Extent3d {
                        width: extent.width,
                        height: extent.height,
                        depth_or_array_layers: 1,
                    },
                );
                Ok(readback)
            },
        )
        .expect("the Host must submit the exact output use and retain its imported lease");

    let (sender, receiver) = mpsc::sync_channel(1);
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + TIMEOUT;
    let mapping = loop {
        if prefix
            .poll()
            .expect("the readback prefix must remain pollable")
        {
            // Prefix settlement does not guarantee a later map callback was serviced.
            host.with_image(output, |device, _, _| device.poll(wgpu::PollType::Poll))
                .expect("the owning Host must admit polling this exact output")
                .expect("service the native readback map callback");
        }
        match receiver.try_recv() {
            Ok(completed) => break completed,
            Err(TryRecvError::Empty) => {
                assert!(Instant::now() < deadline, "native readback timed out");
                thread::sleep(Duration::from_millis(1));
            }
            Err(TryRecvError::Disconnected) => panic!("native readback callback disappeared"),
        }
    };
    mapping.expect("native readback mapping must succeed");
    settle(&prefix);
    {
        let mapped = readback.get_mapped_range(..).expect("readback is mapped");
        assert_eq!(mapped.len() as u64, bytes);
        for row in mapped.chunks_exact(padded_row as usize) {
            for pixel in row[..row_bytes as usize].chunks_exact(4) {
                assert_eq!(pixel, expected.as_slice(), "exact retained output pixels");
            }
        }
    }
    readback.unmap();
}
