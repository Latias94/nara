#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

mod support;

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    WgpuFeatureInstance, WgpuRenderHost, WgpuRenderHostError, WgpuRenderProducer,
    WgpuRenderProgramBuilder, WgpuRenderProgramError, WgpuSubmissionPrefix,
};
use support::{
    features::{Axis, Blur},
    model::*,
    oracle,
    readback::{Readback, Sample},
    workload::{self, HORIZONTAL, Installed, Published, VERTICAL},
};

const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
#[ignore = "requires Windows and explicit WGPU_BACKEND=dx12 plus NARA_WGPU_FORCE_FALLBACK=0 or 1"]
fn invalid_capture_preflight_refuses_before_known_native_resource_budget() {
    require_dx12_selection();
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(0);
    let producer = host.create_producer().unwrap();
    let installed = Installed::new(&mut host, &producer);
    let source = source();
    begin(&mut host, 1);
    let invalid = workload::build(source, 1, Vec::<[f32; 4]>::new().into(), &installed);
    let error = host
        .submit_program(&producer, &invalid.program)
        .unwrap_err();
    assert!(matches!(
        error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::Feature { .. })
    ));
    assert!(error.is_content_rejected());
    assert!(
        !error.is_deferred(),
        "invalid captured shape precedes temporary budget pressure"
    );
    assert_eq!(host.retained_resource_bytes(), 0);
    host = host.with_resource_byte_limit(4 * 1024 * 1024);
    let valid = workload::build(source, 1, oracle::scene(0).into(), &installed);
    let outputs = host.submit_program(&producer, &valid.program).unwrap();
    settle(outputs.submitted_prefix());
    drop(outputs);
    finish(&mut host);
}

#[test]
#[ignore = "requires Windows and explicit WGPU_BACKEND=dx12 plus NARA_WGPU_FORCE_FALLBACK=0 or 1"]
fn independent_features_preserve_exact_a1_while_a2_and_b_execute() {
    require_dx12_selection();
    let mut host = WgpuRenderHost::offscreen();
    let a = host
        .create_producer()
        .expect("admit producer A without a World or window");
    let b = host
        .create_producer()
        .expect("admit independent producer B");
    let a_source = source();
    let b_source = source();
    assert_ne!(a_source, b_source);
    assert_ne!(a.id(), b.id());
    let a_features = Installed::new(&mut host, &a);
    let b_features = Installed::new(&mut host, &b);
    let readback = workload::install(&mut host, &b, Readback);

    begin(&mut host, 1);
    let a1_input = workload::build(a_source, 1, oracle::scene(0).into(), &a_features);
    let a1_group = host
        .submit_program(&a, &a1_input.program)
        .expect("publish the whole A1 group");
    assert_eq!(
        *a1_group
            .report::<Blur>(HORIZONTAL, &a_features.horizontal)
            .unwrap(),
        Axis::Horizontal
    );
    assert_eq!(
        *a1_group
            .report::<Blur>(VERTICAL, &a_features.vertical)
            .unwrap(),
        Axis::Vertical
    );
    assert!(matches!(
        a1_group.report::<Blur>(HORIZONTAL, &a_features.vertical),
        Err(WgpuRenderProgramError::FeatureState)
    ));
    assert!(matches!(
        a1_group.report::<Blur>(HORIZONTAL, &b_features.horizontal),
        Err(WgpuRenderProgramError::FeatureState)
    ));
    let a1 = a1_input.ports.published(&a1_group);
    assert_eq!(
        a1.scene.descriptor().native.format,
        wgpu::TextureFormat::Rgba32Float
    );
    assert_eq!(
        a1.scene.descriptor().transfer,
        nara_render_wgpu::TextureTransfer::Linear
    );
    assert_eq!(
        a1.scene.descriptor().alpha,
        nara_render_wgpu::TextureAlpha::Premultiplied
    );

    begin(&mut host, 2);
    let a2_input = workload::build(a_source, 2, oracle::scene(1).into(), &a_features);
    let a2_group = host
        .submit_program(&a, &a2_input.program)
        .expect("publish independent A2 storage");
    let a2 = a2_input.ports.published(&a2_group);
    assert!(matches!(
        a1_group.texture(&a2_input.ports.scene),
        Err(WgpuRenderProgramError::UnpublishedResource)
    ));
    let cloned_a1_port = a1_input.ports.scene.clone();
    assert_eq!(a1_group.texture(&cloned_a1_port).unwrap().frame_index(), 1);
    assert_eq!(a1.scene.frame_index(), 1);
    assert_eq!(a2.scene.frame_index(), 2);
    assert_eq!(a1.raw.frame_index(), 1);
    assert_eq!(a2.raw.frame_index(), 2);
    assert_eq!(a1.scene.producer_id(), a.id());
    assert_eq!(a2.scene.producer_id(), a.id());

    let b1_input = workload::build(b_source, 1, oracle::scene(3).into(), &b_features);
    let b1_group = host
        .submit_program(&b, &b1_input.program)
        .expect("B has its own capture clock and native state");
    let b1 = b1_input.ports.published(&b1_group);
    drop((a1_group, a2_group, b1_group));

    // No map, blocking drain or explicit prefix poll precedes producer-local close.
    let a_prefix = host
        .retire_producer(&a)
        .expect("freeze A's original submitted prefix");
    assert!(matches!(
        host.submit_program(&a, &a2_input.program),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(host.retire_producer(&a).is_none());

    begin(&mut host, 3);
    assert_sample(&inspect(&mut host, &b, b_source, 2, &a1, &readback), 0);
    settle(&a_prefix);
    begin(&mut host, 4);
    assert_sample(&inspect(&mut host, &b, b_source, 3, &a2, &readback), 1);
    begin(&mut host, 5);
    assert_sample(&inspect(&mut host, &b, b_source, 4, &b1, &readback), 3);

    // B also renders another full compute/draw group after A physically completes.
    begin(&mut host, 6);
    let b2_input = workload::build(b_source, 5, oracle::scene(5).into(), &b_features);
    let b2_group = host
        .submit_program(&b, &b2_input.program)
        .expect("B stays live after A completion");
    let b2 = b2_input.ports.published(&b2_group);
    drop(b2_group);
    begin(&mut host, 7);
    assert_sample(&inspect(&mut host, &b, b_source, 6, &b2, &readback), 5);
    assert!(a1.scene.is_current() && a1.raw.is_current());
    assert!(a_prefix.poll().expect("A completion stays monotonic"));
    assert_window_free(&host);
    settle(&host.retire_producer(&b).expect("freeze B's own prefix"));
    drop((a1, a2, b1, b2));
    assert_eq!(
        host.retained_resource_bytes(),
        0,
        "last output and submitted-use leases release their charges"
    );
    finish(&mut host);
}

#[test]
#[ignore = "requires Windows and explicit WGPU_BACKEND=dx12 plus NARA_WGPU_FORCE_FALLBACK=0 or 1"]
fn private_allocation_refusal_keeps_old_outputs_and_the_same_candidate_can_retry() {
    require_dx12_selection();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let source = source();
    let installed = Installed::new(&mut host, &producer);
    let readback = workload::install(&mut host, &producer, Readback);
    begin(&mut host, 1);
    let a1_input = workload::build(source, 1, oracle::scene(0).into(), &installed);
    let a1_group = host.submit_program(&producer, &a1_input.program).unwrap();
    let a1 = a1_input.ports.published(&a1_group);
    settle(a1_group.submitted_prefix());
    drop(a1_group);
    let retained = 4 * IMAGE_BYTES + 2 * COUNTS_BYTES;
    assert_eq!(host.retained_resource_bytes(), retained);

    // All five local textures and both buffers fit. The private upload allocation is one
    // byte short, so refusal occurs inside Feature preparation after graph realization.
    let attempt_bytes = 6 * IMAGE_BYTES + 2 * COUNTS_BYTES;
    host = host.with_resource_byte_limit(retained + attempt_bytes - 1);
    let a2_input = workload::build(source, 2, oracle::scene(1).into(), &installed);
    begin(&mut host, 2);
    assert!(matches!(
        host.submit_program(&producer, &a2_input.program),
        Err(WgpuRenderHostError::Program(
            WgpuRenderProgramError::ResourceCapacity
        ))
    ));

    host = host.with_resource_byte_limit(4 * 1024 * 1024);
    begin(&mut host, 3);
    let a2_group = host
        .submit_program(&producer, &a2_input.program)
        .expect("retry the exact same immutable program and capture index after refusal");
    let a2 = a2_input.ports.published(&a2_group);
    assert_eq!(a2.scene.frame_index(), 2);
    assert_eq!(a1.scene.frame_index(), 1);
    drop(a2_group);
    begin(&mut host, 4);
    assert_sample(&inspect(&mut host, &producer, source, 3, &a1, &readback), 0);
    begin(&mut host, 5);
    assert_sample(&inspect(&mut host, &producer, source, 4, &a2, &readback), 1);
    assert_window_free(&host);
    settle(
        &host
            .retire_producer(&producer)
            .expect("the original producer retains its latest prefix"),
    );
    drop((a1, a2));
    assert_eq!(host.retained_resource_bytes(), 0);
    finish(&mut host);
}

fn require_dx12_selection() {
    assert!(
        cfg!(target_os = "windows"),
        "this native qualification requires Windows/DX12"
    );
    assert_eq!(
        std::env::var("WGPU_BACKEND").as_deref(),
        Ok("dx12"),
        "explicitly select WGPU_BACKEND=dx12; an absent or different adapter is a failing prerequisite"
    );
    let fallback = std::env::var("NARA_WGPU_FORCE_FALLBACK");
    assert!(
        matches!(fallback.as_deref(), Ok("0") | Ok("1")),
        "explicitly choose NARA_WGPU_FORCE_FALLBACK=0 (hardware policy) or 1 (fallback policy)"
    );
    eprintln!(
        "public render-program consumer: WGPU_BACKEND=dx12, NARA_WGPU_FORCE_FALLBACK={}",
        fallback.unwrap()
    );
}

fn begin(host: &mut WgpuRenderHost, frame: u64) {
    host.begin_frame(frame)
        .expect("the production Host must acquire its selected native GPU; no passing skip");
    assert_window_free(host);
}

fn inspect(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    source: RenderFrameSourceGeneration,
    frame: u64,
    published: &Published,
    instance: &WgpuFeatureInstance<Readback>,
) -> Sample {
    let mut builder = WgpuRenderProgramBuilder::new(source, frame, limits()).unwrap();
    let ports = published.import(&mut builder);
    builder
        .operation(
            1,
            instance,
            Arc::new(ports.clone()),
            &ports.copy_reads(),
            &[],
        )
        .unwrap();
    let program = builder
        .finish(&ports.outputs())
        .expect("a read-only use publishes its exact imported group");
    let outputs = host
        .submit_program(producer, &program)
        .expect("Host admission precedes any report mapping");
    let report = outputs
        .report::<Readback>(1, instance)
        .expect("lookup the original instance's typed report");
    let sample = report.read();
    settle(outputs.submitted_prefix());
    sample
}

fn assert_sample(actual: &Sample, capture: u32) {
    let scene = oracle::scene(capture);
    let blurred = oracle::blurred(&scene);
    let effect = oracle::complement(&blurred);
    let counts = oracle::counts(&scene);
    let frequencies = oracle::frequencies(counts);
    oracle::assert_pixels(&actual.scene, &scene);
    oracle::assert_pixels(&actual.blurred, &blurred);
    oracle::assert_pixels(&actual.effect, &effect);
    assert_eq!(
        actual.raw, counts,
        "all four raw integer bins remain exact after successor normalization"
    );
    assert_eq!(actual.raw.into_iter().sum::<u32>(), WIDTH * HEIGHT);
    for (actual, expected) in actual.frequencies.into_iter().zip(frequencies) {
        assert!(
            actual.is_finite() && (actual - expected).abs() <= 1.0e-6,
            "frequency: actual={actual}, expected={expected}"
        );
    }
    oracle::assert_pixels(&actual.histogram, &oracle::histogram(&effect, frequencies));
    assert!(scene.iter().any(|pixel| pixel[3] == 0.0));
    assert!(scene.iter().any(|pixel| pixel[3] == 0.5));
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
        .expect("poll the original native submission prefix")
    {
        assert!(
            Instant::now() < deadline,
            "native submission prefix timed out"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(prefix.is_complete());
}

fn finish(host: &mut WgpuRenderHost) {
    let deadline = Instant::now() + TIMEOUT;
    while !host
        .poll_retirement()
        .expect("poll final production Host retirement")
    {
        assert!(Instant::now() < deadline, "final Host retirement timed out");
        thread::sleep(Duration::from_millis(1));
    }
}
