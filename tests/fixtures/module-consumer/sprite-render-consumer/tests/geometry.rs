#![forbid(unsafe_code)]

#[path = "support/readback.rs"]
mod readback;
#[path = "support/scene.rs"]
mod scene;

use nara_app::{RuntimeCandidateRetirementState, RuntimeInstance};
use nara_render::{RenderImage2d, RenderImagePacket};
use nara_render_wgpu::{
    WgpuPublishedFrame, WgpuRenderHost, WgpuRenderInput, WgpuRenderProducer, WgpuSceneColor,
};
use nara_sprite_render::SpriteRenderStats;
use scene::{Result, Scenario};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

const WARMUP: usize = 8;
const SAMPLES: usize = 64;

#[derive(Clone, Copy)]
struct Case {
    population: usize,
    views: usize,
    scenario: Scenario,
    bound: bool,
}

#[test]
fn ordinary_module_capture_observes_direct_cell_edits_and_new_view_samples() -> Result<()> {
    for views in [1, 2] {
        for bound in [false, true] {
            for scenario in [Scenario::Static, Scenario::Cell, Scenario::Camera] {
                let dropped = Arc::new(AtomicBool::new(false));
                let mut runtime = scene::build(64, views, scenario, bound, Arc::clone(&dropped))?;
                let result = catch_unwind(AssertUnwindSafe(|| {
                    let a = scene::drive(&mut runtime);
                    let a_state = scene::snapshot(&runtime);
                    let b = scene::drive(&mut runtime);
                    let b_state = scene::snapshot(&runtime);
                    assert_eq!(a.packet().views().len(), views);
                    assert_eq!(
                        b.packet().source_generation(),
                        a.packet().source_generation()
                    );
                    assert!(b.packet().frame_index() > a.packet().frame_index());
                    assert!(a.captured::<nara_render_wgpu::WgpuStockPolicy>().is_some());
                    let stats = runtime.world().resource::<SpriteRenderStats>();
                    assert_eq!(stats.extracted_tile_cells, 64);
                    assert_eq!(stats.extracted_sprites, 1);
                    assert!(stats.queued_items > 0 && stats.queued_items <= 65 * views as u32);
                    if scenario == Scenario::Cell {
                        assert_ne!(a_state.center_red, b_state.center_red);
                    }
                    if scenario == Scenario::Camera {
                        assert_ne!(a_state.camera_x, b_state.camera_x);
                    }
                    a
                }));
                retire(runtime);
                assert!(dropped.load(Ordering::Acquire));
                let a = match result {
                    Ok(input) => input,
                    Err(payload) => resume_unwind(payload),
                };
                assert_eq!(a.packet().views().len(), views);
            }
        }
    }
    Ok(())
}

#[test]
fn adapter_observation_does_not_initialize_an_offscreen_host() {
    let mut host = WgpuRenderHost::offscreen();
    assert!(host.adapter_summary().is_none());
    let producer = host.create_producer().unwrap();
    assert!(host.adapter_summary().is_none());
    assert!(host.retire_producer(&producer).is_none());
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native adapter; complete public pixel and retirement control"]
fn original_tile_captures_and_pixels_survive_edits_views_and_runtime_retirement() -> Result<()> {
    require_adapter_selection();
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(64 * 1024 * 1024);
    let mut sequence = 0;
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
        for views in [1, 2] {
            for bound in [false, true] {
                for scenario in [Scenario::Static, Scenario::Cell, Scenario::Camera] {
                    run_group(
                        &mut host,
                        &mut sequence,
                        Case {
                            population: 64,
                            views,
                            scenario,
                            bound,
                        },
                        4,
                        None,
                    )?;
                }
            }
        }
        Ok(())
    }));
    readback::finish(&mut host);
    match result {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

#[test]
#[ignore = "optimized current-public-stock geometry cost measurement; set an absolute fresh JSONL output path"]
fn measure_current_public_geometry_costs() -> Result<()> {
    if cfg!(debug_assertions) {
        return Err("measure ordinary release builds".into());
    }
    require_adapter_selection();
    let path = PathBuf::from(
        std::env::var_os("NARA_GEOMETRY_COST_OUTPUT").expect("explicit absolute output path"),
    );
    assert!(path.is_absolute());
    let revision =
        std::env::var("NARA_GEOMETRY_COST_REVISION").expect("explicit measured source revision");
    assert!(!revision.is_empty() && revision.len() <= 128);
    let mut output = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(64 * 1024 * 1024);
    assert!(host.adapter_summary().is_none());
    host.begin_frame(1)?;
    let adapter = host
        .adapter_summary()
        .expect("the actual original Host adapter");
    assert_eq!(adapter.backend(), std::env::var("WGPU_BACKEND")?);
    if std::env::var("NARA_WGPU_FORCE_FALLBACK")? == "0" {
        assert_ne!(adapter.device_type(), "cpu");
    }
    serde_json::to_writer(
        &mut output,
        &json!({"measurement":"geometry-cost-environment-v1", "revision":revision, "process":std::process::id(), "warmup":WARMUP, "samples":SAMPLES, "backend":adapter.backend(), "name":adapter.name(), "vendor":adapter.vendor(), "device":adapter.device(), "device_type":adapter.device_type(), "driver":adapter.driver(), "driver_info":adapter.driver_info(), "substrate":"current-public-stock-policy", "scope":"offscreen-geometry-no-hud-serial-prefix-completion"}),
    )?;
    writeln!(output)?;
    let mut sequence = 1;
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
        for population in [64, 4096, 65536] {
            for views in [1, 2] {
                for bound in [false, true] {
                    for scenario in [Scenario::Static, Scenario::Cell, Scenario::Camera] {
                        run_group(
                            &mut host,
                            &mut sequence,
                            Case {
                                population,
                                views,
                                scenario,
                                bound,
                            },
                            SAMPLES,
                            Some(&mut output),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }));
    readback::finish(&mut host);
    match result {
        Ok(result) => result?,
        Err(payload) => resume_unwind(payload),
    }
    output.flush()?;
    println!(
        "GEOMETRY_COST_OK groups=36 samples={} original_host_retired=true",
        36 * SAMPLES
    );
    Ok(())
}

fn require_adapter_selection() {
    assert!(matches!(
        std::env::var("WGPU_BACKEND").as_deref(),
        Ok("dx12" | "vulkan" | "metal")
    ));
    assert!(matches!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0" | "1")
    ));
}

fn retire(runtime: RuntimeInstance) {
    let mut retirement = runtime.begin_retirement();
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
        assert!(
            Instant::now() < deadline,
            "original Runtime retirement deadline"
        );
        retirement.drive_retirement();
        std::thread::yield_now();
    }
}

fn target() -> RenderImagePacket {
    RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        nara_render::Extent2d::new(scene::WIDTH, scene::HEIGHT).unwrap(),
    )
    .unwrap()
}

fn adapt_input(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    source: &WgpuRenderInput,
    target: RenderImagePacket,
    activate: bool,
) -> WgpuRenderInput {
    let input = source.with_image_target(target).unwrap();
    if activate {
        let reservation = host
            .reserve_image_target(producer, input.packet().source_generation(), target)
            .unwrap();
        host.activate_image_target(producer, reservation).unwrap();
    }
    input
}

fn submit(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    input: &WgpuRenderInput,
) -> WgpuPublishedFrame {
    let candidate = host.prepare(producer, input).unwrap();
    host.submit_candidate(producer, candidate)
        .map_err(|failure| failure.error)
        .unwrap()
}

fn run_group(
    host: &mut WgpuRenderHost,
    sequence: &mut u64,
    case: Case,
    samples: usize,
    mut output: Option<&mut dyn Write>,
) -> Result<()> {
    let Case {
        population,
        views,
        scenario,
        bound,
    } = case;
    let dropped = Arc::new(AtomicBool::new(false));
    let mut runtime = scene::build(population, views, scenario, bound, Arc::clone(&dropped))?;
    let producer = match host.create_producer() {
        Ok(producer) => producer,
        Err(error) => {
            retire(runtime);
            return Err(error.into());
        }
    };
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<_> {
        let image = target();
        let original = scene::drive(&mut runtime);
        let original_state = scene::snapshot(&runtime);
        let mut previous = None;
        let mut first = None;
        let mut last_state = original_state;
        for sample in 0..WARMUP + samples {
            let total = Instant::now();
            let input = scene::drive(&mut runtime);
            let drive_ns = total.elapsed().as_nanos();
            let begin = Instant::now();
            drop(previous.take());
            *sequence += 1;
            host.begin_frame(*sequence)?;
            let setup_ns = begin.elapsed().as_nanos();
            let begin = Instant::now();
            let input = adapt_input(host, &producer, &input, image, sample == 0);
            let adaptation_ns = begin.elapsed().as_nanos();
            let begin = Instant::now();
            let candidate = host.prepare(&producer, &input)?;
            let prepare_ns = begin.elapsed().as_nanos();
            let begin = Instant::now();
            let published = host
                .submit_candidate(&producer, candidate)
                .map_err(|failure| failure.error)?;
            let submit_ns = begin.elapsed().as_nanos();
            let cpu_to_submit_ns = total.elapsed().as_nanos();
            let begin = Instant::now();
            readback::settle(published.outputs().submitted_prefix());
            let wait_ns = begin.elapsed().as_nanos();
            let state = scene::snapshot(&runtime);
            let stats = *runtime.world().resource::<SpriteRenderStats>();
            assert_eq!(stats.extracted_tile_cells as usize, population);
            assert_eq!(stats.extracted_sprites, 1);
            assert!(
                stats.queued_items > 0 && stats.queued_items as usize <= (population + 1) * views
            );
            assert_eq!(
                stats.missing_materials
                    + stats.missing_tilesets
                    + stats.invalid_tile_regions
                    + stats.invalid_texture_regions
                    + stats.unprepared_textures,
                0
            );
            if sample == WARMUP + 1 {
                first = Some((published.final_texture::<WgpuSceneColor>()?, state));
            }
            if sample >= WARMUP
                && let Some(output) = output.as_deref_mut()
            {
                let work = host.frame_transaction_stats().native_work();
                let row: Value = json!({
                    "measurement": "geometry-cost-v1",
                    "population": population,
                    "views": views,
                    "material": if bound { "bound" } else { "plain" },
                    "scenario": scenario.name(),
                    "sample": sample-WARMUP,
                    "managed_drive_capture_ns": drive_ns,
                    "host_frame_setup_ns": setup_ns,
                    "target_adaptation_ns": adaptation_ns,
                    "host_prepare_ns": prepare_ns,
                    "host_submit_ns": submit_ns,
                    "cpu_to_submit_ns": cpu_to_submit_ns,
                    "original_prefix_wait_ns": wait_ns,
                    "extracted_cells": stats.extracted_tile_cells,
                    "extracted_sprites": stats.extracted_sprites,
                    "queued_items": stats.queued_items,
                    "batches": stats.batches,
                    "initialized_buffer_calls": work.initialized_buffer_calls,
                    "initialized_buffer_payload_bytes": work.initialized_buffer_payload_bytes,
                    "initialized_buffer_storage_bytes": work.initialized_buffer_storage_bytes,
                    "issued_buffer_write_calls": work.issued_buffer_write_calls,
                    "issued_buffer_write_bytes": work.issued_buffer_write_bytes,
                    "issued_texture_write_calls": work.issued_texture_write_calls,
                    "issued_texture_write_bytes": work.issued_texture_write_bytes,
                    "retained_resource_bytes": host.retained_resource_bytes(),
                    "retained_candidate_metadata_bytes": host.retained_candidate_metadata_bytes()
                });
                serde_json::to_writer(&mut *output, &row)?;
                writeln!(output)?;
            }
            last_state = state;
            previous = Some(published);
        }
        let published = previous.take().unwrap();
        let last = published.final_texture::<WgpuSceneColor>()?;
        assert_pixels(&scene::expected(last_state), &readback::read(host, &last));
        let input = scene::drive(&mut runtime);
        let final_state = scene::snapshot(&runtime);
        *sequence += 1;
        host.begin_frame(*sequence)?;
        let input = adapt_input(host, &producer, &input, image, false);
        let final_frame = submit(host, &producer, &input);
        readback::settle(final_frame.outputs().submitted_prefix());
        let final_output = final_frame.final_texture::<WgpuSceneColor>()?;
        Ok((
            first.unwrap(),
            final_output,
            final_state,
            original,
            original_state,
        ))
    }));
    retire(runtime);
    assert!(
        dropped.load(Ordering::Acquire),
        "original World destruction"
    );
    if let Some(prefix) = host.retire_producer(&producer) {
        readback::settle(&prefix);
    }
    let (first, final_output, final_state, original, original_state) = match result {
        Ok(result) => result?,
        Err(payload) => resume_unwind(payload),
    };
    if scenario != Scenario::Static {
        assert_ne!(
            scene::expected(first.1),
            scene::expected(final_state),
            "retained output must differ from later output"
        );
    }
    assert_pixels(&scene::expected(first.1), &readback::read(host, &first.0));
    assert_pixels(
        &scene::expected(final_state),
        &readback::read(host, &final_output),
    );
    let late_producer = host.create_producer()?;
    let replay = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
        *sequence += 1;
        host.begin_frame(*sequence)?;
        let input = adapt_input(host, &late_producer, &original, target(), true);
        let frame = submit(host, &late_producer, &input);
        readback::settle(frame.outputs().submitted_prefix());
        assert_pixels(
            &scene::expected(original_state),
            &readback::read(host, &frame.final_texture::<WgpuSceneColor>()?),
        );
        if scenario != Scenario::Static {
            assert_ne!(
                scene::expected(original_state),
                scene::expected(final_state),
                "the source changes must distinguish old input from current output"
            );
        }
        Ok(())
    }));
    if let Some(prefix) = host.retire_producer(&late_producer) {
        readback::settle(&prefix);
    }
    match replay {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}

fn assert_pixels(expected: &[[u8; 4]], actual: &[[u8; 4]]) {
    assert_eq!(actual.len(), expected.len());
    if let Some((index, (expected, actual))) = expected
        .iter()
        .zip(actual)
        .enumerate()
        .find(|(_, (expected, actual))| expected != actual)
    {
        panic!(
            "geometry pixel [{},{}]: {actual:?} != {expected:?}",
            index % scene::WIDTH as usize,
            index / scene::WIDTH as usize
        );
    }
}
