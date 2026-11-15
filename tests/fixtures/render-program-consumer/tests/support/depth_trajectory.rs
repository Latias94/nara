use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use nara_render::RenderFrameSourceGeneration;
use nara_render_wgpu::{
    AccessRole, TextureInterpretation, WgpuFeatureInstance, WgpuNativeWorkStats, WgpuRenderHost,
    WgpuRenderHostError, WgpuRenderProducer, WgpuRenderProgramBuilder, WgpuRenderProgramError,
    WgpuSubmissionPrefix,
};

use crate::{
    depth_features::{self as features, Installed, Published, Workload},
    depth_model::*,
    depth_readback::{Observation, Readback, ReadbackCapture},
};

const TIMEOUT: Duration = Duration::from_secs(10);
const RESOURCE_LIMIT: u64 = 16 * 1024 * 1024;

fn selected_adapter() {
    let backend =
        std::env::var("WGPU_BACKEND").expect("explicitly select WGPU_BACKEND=dx12 or vulkan");
    assert!(
        matches!(backend.as_str(), "dx12" | "vulkan"),
        "depth qualification requires exactly one explicit DX12 or Vulkan backend",
    );
    assert!(
        backend != "dx12" || cfg!(target_os = "windows"),
        "DX12 requires Windows"
    );
    let fallback = std::env::var("NARA_WGPU_FORCE_FALLBACK")
        .expect("explicitly select NARA_WGPU_FORCE_FALLBACK=0 or 1");
    assert!(
        matches!(fallback.as_str(), "0" | "1"),
        "invalid adapter policy"
    );
    eprintln!(
        "independent depth/unit consumer: WGPU_BACKEND={backend}, NARA_WGPU_FORCE_FALLBACK={fallback}"
    );
}

fn host(slot: &mut Option<WgpuRenderHost>) -> &mut WgpuRenderHost {
    slot.as_mut()
        .expect("the original Host remains owned through cleanup")
}

fn limit(slot: &mut Option<WgpuRenderHost>, bytes: u64) {
    *slot = slot.take().map(|host| host.with_resource_byte_limit(bytes));
}

fn assert_window_free(host: &WgpuRenderHost) {
    let facts = host.frame_transaction_stats();
    assert_eq!(facts.surface_acquire_attempts(), 0);
    assert_eq!(facts.presents(), 0);
}

fn begin(host: &mut WgpuRenderHost, frame: u64) {
    host.begin_frame(frame)
        .expect("the explicitly requested original adapter must exist; no passing skip");
    let adapter = host
        .runtime_facts()
        .adapter
        .expect("actual selected adapter facts");
    assert_eq!(adapter.backend(), std::env::var("WGPU_BACKEND").unwrap());
    let fallback = std::env::var("NARA_WGPU_FORCE_FALLBACK").unwrap();
    if fallback == "0" {
        assert_ne!(
            adapter.device_type(),
            "cpu",
            "hardware selection excludes a software adapter"
        );
    }
    if frame == 1 {
        eprintln!("depth/unit: adapter={adapter:?}, forced_fallback={fallback}");
    }
    assert_window_free(host);
    assert_eq!(
        host.frame_transaction_stats().native_work(),
        WgpuNativeWorkStats::default()
    );
}

fn assert_draw_work(host: &WgpuRenderHost, before: WgpuNativeWorkStats) {
    assert_eq!(
        host.frame_transaction_stats().native_work(),
        WgpuNativeWorkStats {
            issued_buffer_write_calls: before.issued_buffer_write_calls + 4,
            issued_buffer_write_bytes: before.issued_buffer_write_bytes + 2 * INPUT_BYTES,
            encoded_texture_copy_commands: before.encoded_texture_copy_commands + 2,
            encoded_texture_copy_bytes: before.encoded_texture_copy_bytes
                + COLOR_BYTES
                + DEPTH_BYTES,
            ..before
        },
        "two captured geometry/camera uploads and graph-owned whole color/depth Preserve copies",
    );
}

fn settle(prefix: &WgpuSubmissionPrefix) {
    let deadline = Instant::now() + TIMEOUT;
    while !prefix
        .poll()
        .expect("poll the exact original submitted prefix")
    {
        assert!(
            Instant::now() < deadline,
            "original depth submission prefix timed out"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(prefix.is_complete());
    assert_eq!(prefix.cleanup_fault_count(), 0);
    assert!(
        prefix
            .poll()
            .expect("completed original prefix is monotonic")
    );
}

fn assert_identity(
    published: &Published,
    producer: &WgpuRenderProducer,
    source: RenderFrameSourceGeneration,
    frame: u64,
) {
    for depth in [&published.depth, &published.preserved_depth] {
        assert!(depth.is_current());
        assert_eq!(depth.producer_id(), producer.id());
        assert_eq!(depth.source_generation(), source);
        assert_eq!(depth.frame_index(), frame);
        assert_eq!(depth.descriptor(), depth_descriptor());
        assert_eq!(
            depth.descriptor().interpretation,
            TextureInterpretation::Depth
        );
    }
    for color in [&published.color, &published.preserved_color] {
        assert!(color.is_current());
        assert_eq!(color.producer_id(), producer.id());
        assert_eq!(color.source_generation(), source);
        assert_eq!(color.frame_index(), frame);
        assert_eq!(color.descriptor(), color_descriptor());
    }
    assert!(published.resolved.is_current());
    assert_eq!(published.resolved.producer_id(), producer.id());
    assert_eq!(published.resolved.source_generation(), source);
    assert_eq!(published.resolved.frame_index(), frame);
    assert_eq!(published.resolved.descriptor(), color_descriptor());
}

fn submit(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    workload: &Workload,
    prefixes: &mut Vec<WgpuSubmissionPrefix>,
) -> Published {
    let before = host.frame_transaction_stats().native_work();
    let group = host
        .submit_program(producer, &workload.program)
        .expect("publish the complete independently authored depth program");
    prefixes.push(group.submitted_prefix().clone());
    let published = workload.ports.published(&group);
    assert_identity(
        &published,
        producer,
        workload.program.source_generation(),
        workload.program.frame_index(),
    );
    assert_draw_work(host, before);
    published
}

fn inspect(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    source: RenderFrameSourceGeneration,
    frame: u64,
    published: &Published,
    installed: &Installed,
    readback: &WgpuFeatureInstance<Readback>,
    prefixes: &mut Vec<WgpuSubmissionPrefix>,
) -> Observation {
    let mut builder = WgpuRenderProgramBuilder::new(source, frame, limits()).unwrap();
    let ports = published.import(&mut builder);
    let original = builder
        .texture::<ResolvedDepth>(color_descriptor())
        .unwrap();
    let preserved = builder
        .texture::<ResolvedDepth>(color_descriptor())
        .unwrap();
    let capture = ReadbackCapture {
        color: [ports.color.clone(), ports.preserved_color.clone()],
        depth: [ports.resolved.clone(), original.clone(), preserved.clone()],
    };
    builder
        .operation(
            1,
            readback,
            Arc::new(capture.clone()),
            &[
                capture.color[0].read(AccessRole::CopySource),
                capture.color[1].read(AccessRole::CopySource),
                capture.depth[0].read(AccessRole::CopySource),
                capture.depth[1].read(AccessRole::CopySource),
                capture.depth[2].read(AccessRole::CopySource),
            ],
            &[],
        )
        .expect("copy only declared float color resources on the original encoder");
    features::resolve(
        &mut builder,
        30,
        &installed.resolve,
        &ports.preserved_depth,
        &preserved,
    );
    features::resolve(
        &mut builder,
        40,
        &installed.resolve,
        &ports.depth,
        &original,
    );
    let program = builder
        .finish(&ports.outputs())
        .expect("publish the exact original imported output group");
    let before = host.frame_transaction_stats().native_work();
    let group = host
        .submit_program(producer, &program)
        .expect("sample imported original and preserved depth leases");
    prefixes.push(group.submitted_prefix().clone());
    let imported = ports.published(&group);
    assert_eq!(imported.depth.producer_id(), published.depth.producer_id());
    assert_eq!(
        imported.depth.source_generation(),
        published.depth.source_generation()
    );
    assert_eq!(imported.depth.frame_index(), published.depth.frame_index());
    assert_eq!(imported.color.frame_index(), published.color.frame_index());
    let observation = group
        .report::<Readback>(1, readback)
        .expect("original typed readback report")
        .read();
    settle(group.submitted_prefix());
    assert_eq!(host.frame_transaction_stats().native_work(), before);
    assert_window_free(host);
    observation
}

fn refused_capture(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    workload: &Workload,
    retained: u64,
) {
    for _ in 0..2 {
        let error = host
            .submit_program(producer, &workload.program)
            .unwrap_err();
        assert!(matches!(
            error,
            WgpuRenderHostError::Program(WgpuRenderProgramError::Feature { .. })
        ));
        assert!(error.is_content_rejected() && !error.is_deferred());
        assert_eq!(host.retained_resource_bytes(), retained);
        assert_eq!(
            host.frame_transaction_stats().native_work(),
            WgpuNativeWorkStats::default()
        );
    }
}

fn refused_capacity(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    workload: &Workload,
    retained: u64,
) {
    for _ in 0..2 {
        assert!(matches!(
            host.submit_program(producer, &workload.program),
            Err(WgpuRenderHostError::Program(
                WgpuRenderProgramError::ResourceCapacity
            ))
        ));
        assert_eq!(host.retained_resource_bytes(), retained);
        assert_eq!(
            host.frame_transaction_stats().native_work(),
            WgpuNativeWorkStats::default()
        );
    }
}

fn refused_after_upload(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    workload: &Workload,
    retained: u64,
    prefixes: &mut Vec<WgpuSubmissionPrefix>,
) {
    for _ in 0..2 {
        let before = host.frame_transaction_stats().native_work();
        assert!(matches!(
            host.submit_program(producer, &workload.program),
            Err(WgpuRenderHostError::Program(
                WgpuRenderProgramError::ResourceCapacity
            ))
        ));
        let expected = WgpuNativeWorkStats {
            issued_buffer_write_calls: before.issued_buffer_write_calls + 2,
            issued_buffer_write_bytes: before.issued_buffer_write_bytes + INPUT_BYTES,
            encoded_texture_copy_commands: before.encoded_texture_copy_commands + 2,
            encoded_texture_copy_bytes: before.encoded_texture_copy_bytes
                + COLOR_BYTES
                + DEPTH_BYTES,
            ..before
        };
        assert_eq!(host.frame_transaction_stats().native_work(), expected);
        let prefix = host
            .producer_submission_prefix(producer)
            .expect("the failed preparation retains its own original queued-upload prefix");
        prefixes.push(prefix.clone());
        settle(&prefix);
        assert_eq!(host.retained_resource_bytes(), retained);
        assert_eq!(host.frame_transaction_stats().native_work(), expected);
        assert_eq!(host.cleanup_fault_count(), 0);
    }
}

fn trajectory(
    slot: &mut Option<WgpuRenderHost>,
    producers: &mut Vec<WgpuRenderProducer>,
    prefixes: &mut Vec<WgpuSubmissionPrefix>,
) {
    producers.push(host(slot).create_producer().unwrap());
    producers.push(host(slot).create_producer().unwrap());
    let a = &producers[0];
    let b = &producers[1];
    let a_source = source();
    let b_source = source();
    assert_ne!(a_source, b_source);
    assert_ne!(a.id(), b.id());
    let a_features = Installed::new(host(slot), a);
    let b_features = Installed::new(host(slot), b);
    let readback = features::install(host(slot), b, Readback);

    let mut authored = Scene::meters(DrawOrder::NearFirst);
    let original_scene = Arc::new(authored.clone());
    let delayed_a = features::build(a_source, 1, Arc::clone(&original_scene), &a_features);
    authored.camera.eye_m[0] = 0.25;
    let changed_scene = Arc::new(authored.clone());
    let changed_b = features::build(b_source, 1, Arc::clone(&changed_scene), &b_features);
    authored.camera.near_m = f64::NAN;
    authored.meters_per_unit = f64::NAN;
    authored.quads[0].positions[0][0] = f32::NAN;
    assert!(authored.validate().is_err());
    assert_eq!(*original_scene, Scene::meters(DrawOrder::NearFirst));
    assert_eq!(changed_scene.camera.eye_m[0], 0.25);
    assert_eq!(changed_scene.meters_per_unit, 1.0);
    assert_eq!(changed_scene.quads[0].positions[0], [-0.75, -0.5, 2.0]);
    assert_ne!(reference(&original_scene), reference(&changed_scene));

    begin(host(slot), 1);
    let b1 = submit(host(slot), b, &changed_b, prefixes);
    let original_b_prefix = prefixes.last().unwrap().clone();
    settle(&original_b_prefix);
    let retained = host(slot).retained_resource_bytes();
    assert_eq!(retained, GRAPH_BYTES);

    begin(host(slot), 2);
    limit(slot, retained);
    let mut invalid_unit = (*original_scene).clone();
    invalid_unit.meters_per_unit = f64::NAN;
    let invalid_unit = features::build(a_source, 1, Arc::new(invalid_unit), &a_features);
    refused_capture(host(slot), a, &invalid_unit, retained);
    let mut invalid_camera = (*original_scene).clone();
    invalid_camera.camera.near_m = invalid_camera.camera.far_m;
    let invalid_camera = features::build(a_source, 1, Arc::new(invalid_camera), &a_features);
    refused_capture(host(slot), a, &invalid_camera, retained);
    limit(slot, retained + GRAPH_BYTES - 1);
    refused_capacity(host(slot), a, &delayed_a, retained);
    limit(slot, retained + GRAPH_BYTES + INPUT_BYTES - 1);
    refused_capacity(host(slot), a, &delayed_a, retained);
    limit(slot, retained + GRAPH_BYTES + 2 * INPUT_BYTES - 1);
    refused_after_upload(host(slot), a, &delayed_a, retained, prefixes);
    limit(slot, RESOURCE_LIMIT);
    let a1 = submit(host(slot), a, &delayed_a, prefixes);
    let original_a_prefix = prefixes.last().unwrap().clone();
    assert_eq!(delayed_a.program.frame_index(), 1);
    assert_eq!(delayed_a.program.source_generation(), a_source);

    begin(host(slot), 3);
    let a1_before = inspect(
        host(slot),
        b,
        b_source,
        2,
        &a1,
        &b_features,
        &readback,
        prefixes,
    );
    a1_before.assert_reference(&original_scene);
    settle(&original_a_prefix);

    let cm_scene = Arc::new(Scene::centimeters(DrawOrder::NearFirst));
    let cm_a = features::build(a_source, 2, Arc::clone(&cm_scene), &a_features);
    begin(host(slot), 4);
    let a_cm = submit(host(slot), a, &cm_a, prefixes);
    let reverse_scene = Arc::new(Scene::meters(DrawOrder::FarFirst));
    let reverse_a = features::build(a_source, 3, Arc::clone(&reverse_scene), &a_features);
    begin(host(slot), 5);
    let a_reverse = submit(host(slot), a, &reverse_a, prefixes);
    let cm_reverse_scene = Arc::new(Scene::centimeters(DrawOrder::FarFirst));
    let cm_reverse_a = features::build(a_source, 4, Arc::clone(&cm_reverse_scene), &a_features);
    begin(host(slot), 6);
    let a_cm_reverse = submit(host(slot), a, &cm_reverse_a, prefixes);

    let a_retirement = host(slot)
        .retire_producer(a)
        .expect("freeze A's original last submitted prefix");
    prefixes.push(a_retirement.clone());
    assert!(host(slot).retire_producer(a).is_none());
    assert!(host(slot).producer_submission_prefix(a).is_none());
    assert!(matches!(
        host(slot).submit_program(a, &delayed_a.program),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    begin(host(slot), 7);
    let a1_after = inspect(
        host(slot),
        b,
        b_source,
        3,
        &a1,
        &b_features,
        &readback,
        prefixes,
    );
    a1_after.assert_reference(&original_scene);
    assert_eq!(
        a1_after, a1_before,
        "retired A's color and sampled depth retain every original byte"
    );
    settle(&a_retirement);

    for (host_frame, capture_frame, published, scene) in [
        (8, 4, &a_cm, &cm_scene),
        (9, 5, &a_reverse, &reverse_scene),
        (10, 6, &a_cm_reverse, &cm_reverse_scene),
    ] {
        begin(host(slot), host_frame);
        let actual = inspect(
            host(slot),
            b,
            b_source,
            capture_frame,
            published,
            &b_features,
            &readback,
            prefixes,
        );
        actual.assert_reference(scene);
        assert_eq!(
            actual, a1_before,
            "meter/centimeter geometry and inverted draw order agree byte exactly"
        );
        assert!(original_a_prefix.poll().unwrap());
        assert!(original_b_prefix.poll().unwrap());
    }
    begin(host(slot), 11);
    let b1_before = inspect(
        host(slot),
        b,
        b_source,
        7,
        &b1,
        &b_features,
        &readback,
        prefixes,
    );
    b1_before.assert_reference(&changed_scene);
    assert_ne!(
        b1_before.color[0], a1_before.color[0],
        "changed captured camera changes occlusion"
    );
    assert_ne!(
        b1_before.resolved_depth[0], a1_before.resolved_depth[0],
        "changed captured camera changes depth coverage"
    );

    let mut changed_reverse_scene = (*changed_scene).clone();
    changed_reverse_scene.order = DrawOrder::FarFirst;
    let changed_reverse_scene = Arc::new(changed_reverse_scene);
    let changed_reverse_b =
        features::build(b_source, 8, Arc::clone(&changed_reverse_scene), &b_features);
    begin(host(slot), 12);
    let b2 = submit(host(slot), b, &changed_reverse_b, prefixes);
    begin(host(slot), 13);
    let b2_observation = inspect(
        host(slot),
        b,
        b_source,
        9,
        &b2,
        &b_features,
        &readback,
        prefixes,
    );
    b2_observation.assert_reference(&changed_reverse_scene);
    assert_eq!(b2_observation, b1_before);
    assert!(
        a_retirement.poll().unwrap(),
        "later B work cannot extend A's frozen retirement"
    );

    begin(host(slot), 14);
    let a1_final = inspect(
        host(slot),
        b,
        b_source,
        10,
        &a1,
        &b_features,
        &readback,
        prefixes,
    );
    a1_final.assert_reference(&original_scene);
    assert_eq!(
        a1_final, a1_before,
        "reimport old A after B continues real native drawing"
    );
    assert_identity(&a1, a, a_source, 1);
    assert_identity(&a_cm, a, a_source, 2);
    assert_identity(&a_reverse, a, a_source, 3);
    assert_identity(&a_cm_reverse, a, a_source, 4);
    assert_identity(&b1, b, b_source, 1);
    assert_identity(&b2, b, b_source, 8);
    let b_retirement = host(slot)
        .retire_producer(b)
        .expect("freeze B's own final sampling prefix");
    prefixes.push(b_retirement);
    for prefix in prefixes.iter() {
        settle(prefix);
    }
    drop((a1, a_cm, a_reverse, a_cm_reverse, b1, b2));
    drop((
        delayed_a,
        cm_a,
        reverse_a,
        cm_reverse_a,
        changed_b,
        changed_reverse_b,
        invalid_unit,
        invalid_camera,
    ));
    assert_eq!(
        host(slot).retained_resource_bytes(),
        0,
        "release all original output and private allocation leases"
    );
    assert_eq!(host(slot).cleanup_fault_count(), 0);
    assert_window_free(host(slot));
    eprintln!(
        "depth/unit: native_draw_groups=6, imported_readbacks=8, post_upload_refusals=2, graph_bytes={GRAPH_BYTES}, prefix_observations={}, retained_bytes=0",
        prefixes.len(),
    );
}

fn panic_text(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic")
}

fn cleanup_step(failures: &mut Vec<String>, label: &str, action: impl FnOnce()) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(action)) {
        failures.push(format!("{label}: {}", panic_text(payload.as_ref())));
    }
}

pub fn run() {
    selected_adapter();
    let mut slot = Some(WgpuRenderHost::offscreen().with_resource_byte_limit(RESOURCE_LIMIT));
    let mut producers = Vec::new();
    let mut prefixes = Vec::new();
    let result = catch_unwind(AssertUnwindSafe(|| {
        trajectory(&mut slot, &mut producers, &mut prefixes)
    }));
    let mut failures = Vec::new();
    for producer in &producers {
        cleanup_step(&mut failures, "retire original producer", || {
            if let Some(prefix) = host(&mut slot).retire_producer(producer) {
                prefixes.push(prefix);
            }
        });
    }
    for prefix in &prefixes {
        cleanup_step(&mut failures, "drain original prefix", || settle(prefix));
    }
    cleanup_step(&mut failures, "drain original Host", || {
        let deadline = Instant::now() + TIMEOUT;
        while !host(&mut slot)
            .poll_retirement()
            .expect("poll original Host retirement")
        {
            assert!(
                Instant::now() < deadline,
                "final depth Host retirement timed out"
            );
            thread::sleep(Duration::from_millis(1));
        }
    });
    cleanup_step(&mut failures, "verify original resource release", || {
        assert_eq!(host(&mut slot).retained_resource_bytes(), 0);
        assert_eq!(host(&mut slot).cleanup_fault_count(), 0);
        assert_window_free(host(&mut slot));
    });
    drop(prefixes);
    drop(producers);
    cleanup_step(&mut failures, "drop original Host", || drop(slot.take()));
    if let Err(payload) = result {
        for failure in failures {
            eprintln!("depth trajectory secondary cleanup failure: {failure}");
        }
        resume_unwind(payload);
    }
    assert!(
        failures.is_empty(),
        "depth trajectory cleanup failed: {failures:?}"
    );
}
