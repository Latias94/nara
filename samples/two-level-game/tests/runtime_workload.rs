//! Physical native acceptance for the ordinary active-gameplay measurement executable.

#![cfg(all(
    windows,
    feature = "desktop",
    feature = "cost-measurement",
    feature = "development"
))]
#![forbid(unsafe_code)]

use std::{
    fs,
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::project_tooling::process::{
    ProcessLogLimits, SupervisedProcessCommand, SupervisedProcessOwner, SupervisedProcessState,
};

#[test]
#[ignore = "three real hardware DX12 gameplay runs; execute serially on an interactive Windows desktop"]
fn ordinary_active_gameplay_records_original_frames_and_physically_retires_every_run() {
    execute(false, None);
}

#[test]
#[ignore = "requires a hardware DX12 desktop; proves original native cleanup after an injected observer panic"]
fn a_registered_observer_panic_retires_the_original_window_before_failure_returns() {
    execute(true, None);
}

#[test]
#[ignore = "nine complete native hardware DX12 game lifetimes with authored 64/4096/16384-cell content and retained-input GPU replay; execute serially"]
fn authored_tile_populations_complete_original_travel_and_retained_input_replay() {
    for population in [64, 4096, 16_384] {
        execute(false, Some(population));
    }
}

#[test]
#[ignore = "requires hardware DX12; proves original native and copied-source custody on a Tilemap observer failure"]
fn an_authored_tile_observer_panic_retires_original_native_owners() {
    execute(true, Some(4096));
}

fn execute(fail_observer: bool, population: Option<usize>) {
    assert_eq!(std::env::var("WGPU_BACKEND").as_deref(), Ok("dx12"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "nara-runtime-workload-{}-{stamp}",
        std::process::id(),
    ));
    fs::create_dir(&directory).unwrap();
    let output_path = directory.join("report.json");
    let mut command = SupervisedProcessCommand::new(
        env!("CARGO_BIN_EXE_runtime_workload"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .environment("WGPU_BACKEND", "dx12")
    .environment("NARA_WGPU_FORCE_FALLBACK", "0")
    .environment("NARA_RUNTIME_WORKLOAD_OUTPUT", output_path.as_os_str());
    if fail_observer {
        command = command.argument("--fail-observer");
    }
    if let Some(population) = population {
        command = command.argument("--tiles").argument(population.to_string());
    }
    let mut process = SupervisedProcessOwner::spawn(
        &command,
        ProcessLogLimits::try_from_counts(2048, 1024 * 1024).unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut timed_out = false;
    loop {
        process.advance(Instant::now()).unwrap();
        if !timed_out && Instant::now() >= deadline {
            timed_out = true;
            if process.state() != SupervisedProcessState::Complete {
                process.request_terminate(Instant::now()).unwrap();
            }
        }
        if process.state() == SupervisedProcessState::Complete {
            break;
        }
        if timed_out && Instant::now() >= deadline + Duration::from_secs(15) {
            std::mem::forget(process);
            panic!(
                "original workload child still retains containment at {}",
                directory.display()
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
    let logs = process
        .logs()
        .iter()
        .map(|entry| String::from_utf8_lossy(entry.bytes()).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let receipt = process.cleanup_receipt().unwrap();
    assert!(
        !timed_out,
        "workload deadline expired; report at {}: {logs}",
        directory.display()
    );
    assert_eq!(
        receipt.exit().success(),
        !fail_observer,
        "unexpected workload exit; report at {}: {receipt:?}\n{logs}",
        directory.display()
    );
    assert!(receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained());
    assert!(!receipt.force_killed() && !process.output_loss_observed());
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    assert_eq!(report["format"], "two-level-active-runtime-workload/v1");
    assert_eq!(report["accepted"], !fail_observer);
    assert_eq!(report["injected_observer_failure"], fail_observer);
    if fail_observer {
        assert!(
            logs.contains("injected registered workload observer failure"),
            "{logs}"
        );
        assert_eq!(report["runs"].as_array().unwrap().len(), 1);
        assert_eq!(
            report["runs"][0]["scene_work"]["status"],
            if cfg!(feature = "scene-work-observation") {
                "complete"
            } else {
                "disabled"
            }
        );
        assert!(
            report["runs"][0]["failure"]
                .as_str()
                .unwrap()
                .contains("protected boundary")
        );
        assert_eq!(report["runs"][0]["cleanup"]["native_destroyed"], true);
        assert_eq!(
            report["runs"][0]["cleanup"]["physical_event_loop_recovered"],
            true
        );
        if population.is_some() {
            assert_eq!(report["runs"][0]["copied_source_removed"], true);
            assert!(report["runs"][0]["copied_source_cleanup_failure"].is_null());
        }
    } else {
        assert!(logs.contains("RUNTIME_WORKLOAD_OK runs=3 gameplay_verified=true telemetry_complete=true original_native_retired=true"), "{logs}");
        assert_eq!(report["runs"].as_array().unwrap().len(), 3);
        for run in report["runs"].as_array().unwrap() {
            assert_eq!(run["cleanup"]["normal_exit"], true);
            assert_eq!(run["cleanup"]["telemetry_complete"], true);
            assert_eq!(run["cleanup"]["physical_event_loop_recovered"], true);
            assert_eq!(run["cleanup"]["native_destroyed"], true);
            assert_eq!(run["startup_frozen_after_first_presentation"], true);
            #[cfg(feature = "scene-work-observation")]
            {
                let work = &run["scene_work"];
                assert_eq!(work["enabled"], true);
                assert_eq!(work["status"], "complete");
                assert_eq!(work["target"], "nara::scene::work");
                assert!(work["total_calls"].as_u64().unwrap() > 0);
                assert!(work["nested_span_wall_sum_ns"].as_u64().unwrap() > 0);
                assert_eq!(work["unfinished_entries"], 0);
                assert!(
                    work["phases"]["scene_expand"]["worker_thread_calls"]
                        .as_u64()
                        .unwrap()
                        > 0
                );
                assert!(
                    work["phases"]["scene_flat_admission"]["worker_thread_calls"]
                        .as_u64()
                        .unwrap()
                        > 0
                );
                assert_eq!(
                    work["phases"]["scene_prepare_unbound"]["owner_thread_calls"],
                    6
                );
                assert_eq!(
                    work["phases"]["scene_prepare_unbound"]["worker_thread_calls"],
                    0
                );
                assert_eq!(work["phases"]["scene_publish"]["owner_thread_calls"], 6);
                assert_eq!(work["phases"]["scene_publish"]["worker_thread_calls"], 0);
                assert_eq!(work["phases"]["scene_prepare_unbound"]["calls"], 6);
                assert_eq!(work["phases"]["scene_publish"]["calls"], 6);
                for phase in [
                    "project_content_read",
                    "scene_source_decode",
                    "scene_decode",
                    "scene_expand",
                    "scene_program_composition",
                    "scene_component_materialization",
                    "scene_reference_closure",
                    "scene_flat_admission",
                    "scene_prepare_unbound",
                    "scene_bind",
                    "scene_publish",
                ] {
                    assert!(
                        work["phases"][phase]["calls"].as_u64().unwrap() > 0,
                        "missing Scene work span {phase}: {work}"
                    );
                }
                for phase in ["project_content_read", "scene_source_decode"] {
                    for thread in ["owner_thread_calls", "worker_thread_calls"] {
                        assert!(
                            work["phases"][phase][thread].as_u64().unwrap() > 0,
                            "missing source work {phase}/{thread}: {work}"
                        );
                    }
                }
                if population == Some(16_384) {
                    assert!(
                        work["phases"]["prefab_source_decode"]["worker_thread_calls"]
                            .as_u64()
                            .unwrap()
                            > 0,
                        "missing source Prefab worker decode: {work}"
                    );
                }
            }
            let startup = &run["startup"]["operation_wall_ns"];
            assert_eq!(startup.as_object().unwrap().len(), 5);
            assert!(
                startup["native_polling_to_first_presentation"]
                    .as_u64()
                    .unwrap()
                    >= startup["first_native_poll"].as_u64().unwrap()
            );
            let frames = run["observed_trace"].as_array().unwrap();
            assert_eq!(&run["first_presentation"], frames.first().unwrap());
            assert_eq!(
                &run["first_initialized_presentation"],
                frames
                    .iter()
                    .find(|frame| frame["initialized"] == true)
                    .unwrap()
            );
            let entry = run["entry_to_first_presentation_observation_wall_ns"]
                .as_u64()
                .unwrap();
            if let Some(wall) = frames[0]["app_to_post_host_wall_ns"].as_u64() {
                assert!(entry >= wall);
            }
            assert_eq!(run["runtime_phase_order"].as_array().unwrap().len(), 5);
            assert_eq!(run["namespace_phase_order"].as_array().unwrap().len(), 5);
            let mut adapter_requests = 0;
            let mut device_requests = 0;
            let mut observed_namespaces = 0;
            for frame in frames {
                let wall = &frame["runtime_wall"];
                if frame["initialized"] != true && wall.is_null() {
                    continue;
                }
                let phases = wall["phase_wall_ns"].as_array().unwrap();
                assert_eq!(phases.len(), 5);
                let total = phases
                    .iter()
                    .try_fold(0_u64, |sum, phase| sum.checked_add(phase.as_u64().unwrap()))
                    .unwrap();
                assert_eq!(wall["measured_phase_wall_ns"].as_u64(), Some(total));
                if let Some(frame_wall) = frame["app_to_post_host_wall_ns"].as_u64() {
                    assert_eq!(
                        total.checked_add(wall["other_app_frame_wall_ns"].as_u64().unwrap()),
                        Some(frame_wall),
                    );
                } else {
                    assert!(wall["other_app_frame_wall_ns"].is_null());
                }
                assert_eq!(frame["issued_native_work"].as_object().unwrap().len(), 11);
                let namespace = &wall["namespace"];
                let parts = namespace["phase_wall_ns"].as_array().unwrap();
                assert_eq!(parts.len(), 5);
                let nested_sum = parts
                    .iter()
                    .try_fold(0_u64, |sum, value| {
                        if value.is_null() {
                            Some(sum)
                        } else {
                            sum.checked_add(value.as_u64().unwrap())
                        }
                    })
                    .unwrap();
                assert_eq!(
                    namespace["measured_phase_wall_ns"].as_u64(),
                    Some(nested_sum)
                );
                assert_eq!(
                    nested_sum.checked_add(namespace["other_namespace_wall_ns"].as_u64().unwrap()),
                    phases[0].as_u64(),
                );
                for index in [0, 1, 4] {
                    assert!(parts[index].as_u64().is_some());
                }
                assert_eq!(parts[2].is_null(), parts[3].is_null());
                if observed_namespaces == 0 {
                    assert!(
                        parts[2].as_u64().is_some(),
                        "fresh original namespace must retain its Adapter request"
                    );
                    assert!(
                        parts[3].as_u64().is_some(),
                        "fresh original namespace must retain its Device request"
                    );
                } else {
                    assert!(
                        parts[2].is_null(),
                        "Ready namespace must not invent another Adapter request"
                    );
                    assert!(
                        parts[3].is_null(),
                        "Ready namespace must not invent another Device request"
                    );
                }
                observed_namespaces += 1;
                adapter_requests += usize::from(!parts[2].is_null());
                device_requests += usize::from(!parts[3].is_null());
            }
            assert_eq!(
                adapter_requests, 1,
                "one original Adapter request across the complete native lifetime"
            );
            assert_eq!(
                device_requests, 1,
                "one original Device request across the complete native lifetime"
            );
            if let Some(population) = population {
                assert_eq!(run["authored_tile_cells"], population);
                assert_eq!(run["old_scene_receipts_checked"], 5);
                assert_eq!(run["observed_source_views_retired"], 6);
                assert_eq!(run["copied_source_removed"], true);
                let replay = &run["tile_replay"];
                assert_eq!(replay["accepted"], true);
                assert_eq!(replay["population"], population);
                assert_eq!(
                    replay["original_world_and_sources_retired_before_replay"],
                    true
                );
                assert_eq!(
                    replay["retained_outputs_exact_after_later_publication_and_producer_retirement"],
                    true
                );
                assert_eq!(replay["frames"].as_array().unwrap().len(), 3);
                for replayed in replay["frames"].as_array().unwrap() {
                    assert_eq!(replayed["original_source_and_world_retired"], true);
                    assert_eq!(replayed["replay_source_matches_original"], true);
                    assert_eq!(replayed["submitted_prefix_complete"], true);
                    assert_eq!(replayed["reread_exact_bytes_equal"], true);
                    assert_eq!(replayed["reread_prefix_complete"], true);
                    assert!(
                        replayed["pixels"]["ground_tile_witness_count"]
                            .as_u64()
                            .unwrap()
                            > 0
                    );
                    assert_eq!(replayed["pixels"]["background_pixels"], 36);
                }
                let cleanup = &replay["cleanup"];
                assert_eq!(cleanup["original_producers_retired"], true);
                assert_eq!(cleanup["original_submission_prefixes_complete"], true);
                assert_eq!(cleanup["original_host_retired"], true);
                assert_eq!(cleanup["retained_resource_bytes"], 0);
                assert_eq!(cleanup["retained_candidate_metadata_bytes"], 0);
                assert_eq!(cleanup["cleanup_fault_count"], 0);
                assert_eq!(cleanup["callback_abandoned"], false);
                for frame in frames.iter().filter(|frame| {
                    frame["initialized"] == true
                        && frame["gameplay"]["phase"] == "Running"
                        && frame["gameplay"]["restoring"] != true
                }) {
                    let content = &frame["content_and_render"];
                    assert_eq!(content["authored_tile_cells"], population);
                    assert_eq!(content["extraction"]["tile_cells"], population);
                    assert_eq!(content["render_frame"], frame["render_frame"]);
                    assert_eq!(content["active_scene_instances"], 1);
                    assert_eq!(
                        content["successful_drawing"]["quads"],
                        content["extraction"]["queued_items"]
                    );
                }
            }
        }
        if let Some(evidence) = std::env::var_os("NARA_RUNTIME_WORKLOAD_EVIDENCE") {
            let mut evidence = std::path::PathBuf::from(evidence);
            if let Some(population) = population {
                evidence = evidence.with_file_name(format!(
                    "{}-tiles-{population}.json",
                    evidence.file_stem().unwrap().to_str().unwrap()
                ));
            }
            assert!(evidence.is_absolute());
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(evidence)
                .unwrap()
                .write_all(&fs::read(&output_path).unwrap())
                .unwrap();
        }
    }
    fs::remove_file(output_path).unwrap();
    fs::remove_dir(directory).unwrap();
}
