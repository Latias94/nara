//! Finite real-build cost observations through the qualified production desktop workflow.

#![cfg(all(feature = "desktop", windows, feature = "development"))]
#![forbid(unsafe_code)]

#[path = "support/development_host.rs"]
#[allow(dead_code)]
mod acceptance;
#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod support;

use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::{Duration, Instant},
};

use nara::project_tooling::cargo_build::build_executable;
use serde_json::{Value, json};
use two_level_game::{
    development::{
        DevelopmentHost, DevelopmentPlacement, DevelopmentTimingPhase, DevelopmentTimingSnapshot,
        ExecutionState, InputActivation, RetirementStatus, protocol,
    },
    resume::ResumeState,
};

use acceptance::{
    assert_clean_retirement, assert_preserved, assert_probe_was_discarded, close_to_completion,
    expected_held_tick, finish_retirements, movement, probe_steps, walk_to_score,
};
use support::{Fixture, GAMEPLAY};

const RETAINED: usize = 5;

fn remember_sources(root: &std::path::Path, relative: &str, output: &mut BTreeMap<String, String>) {
    for entry in std::fs::read_dir(root.join(relative)).unwrap() {
        let entry = entry.unwrap();
        assert!(!entry.file_type().unwrap().is_symlink());
        let path = format!("{relative}/{}", entry.file_name().to_str().unwrap());
        if entry.file_type().unwrap().is_dir() {
            remember_sources(root, &path, output);
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "rs")
        {
            output.insert(path, std::fs::read_to_string(entry.path()).unwrap());
        }
    }
}

fn nanos(duration: Duration) -> u64 {
    duration.as_nanos().try_into().unwrap()
}

fn phases(before: DevelopmentTimingSnapshot, after: DevelopmentTimingSnapshot) -> Vec<Value> {
    DevelopmentTimingPhase::ALL
        .into_iter()
        .filter_map(|phase| {
            let before = before.get(phase);
            let after = after.get(phase);
            let operations = after.count - before.count;
            (operations != 0).then(|| {
                json!({
                    "phase": format!("{phase:?}"),
                    "operations": operations,
                    "failures": after.failures - before.failures,
                    "inclusive_elapsed_ns": nanos(after.total_elapsed - before.total_elapsed),
                    "cumulative_maximum_ns": nanos(after.max_elapsed),
                })
            })
        })
        .collect()
}

fn prepare_playable(host: &mut DevelopmentHost) -> ResumeState {
    walk_to_score(host, 10);
    host.step(vec![movement(false)]).unwrap();
    host.step(vec![protocol::SemanticCommand::Marker]).unwrap();
    host.step(vec![protocol::SemanticCommand::Travel]).unwrap();
    walk_to_score(host, 20);
    host.step(vec![protocol::SemanticCommand::Marker]).unwrap();
    let state = host.capture().unwrap();
    assert_eq!(state.progress.score, 20);
    assert_eq!(state.progress.markers.len(), 2);
    assert_eq!(state.held, [false, true, false, false]);
    state
}

fn return_to_isolated_hold(host: &mut DevelopmentHost) -> ResumeState {
    host.pause_active().unwrap();
    host.isolate_active_input().unwrap();
    host.step(vec![movement(true)]).unwrap();
    let state = host.capture().unwrap();
    assert_eq!(state.held, [false, true, false, false]);
    state
}

#[test]
#[ignore = "real nested builds and hardware desktop cost observations; run serially with physical A/D/W/S Up and an explicit NARA_DEVELOPMENT_COST_OUTPUT"]
fn production_desktop_records_complete_no_edit_body_and_structure_costs() {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let output = std::path::PathBuf::from(
        std::env::var_os("NARA_DEVELOPMENT_COST_OUTPUT")
            .expect("supply an explicit new JSON result path"),
    );
    assert!(output.is_absolute() && !output.exists());
    assert!(output.parent().unwrap().is_dir());
    let fixture = Fixture::new();
    let initial_build = Instant::now();
    let executable = build_executable(&fixture.desktop_request(), |_| Ok(())).unwrap();
    let initial_build_ns = nanos(initial_build.elapsed());
    let mut original = BTreeMap::new();
    remember_sources(&fixture.project, "src", &mut original);
    let source_a = fixture.read(GAMEPLAY);
    let mut samples = Vec::new();
    for class in ["no_edit", "body", "structure"] {
        for (path, text) in &original {
            fixture.write(path, text);
        }
        let parent = fixture.directory(&format!("cost-{class}"));
        let mut host = DevelopmentHost::with_placement(
            &fixture.project,
            &parent,
            DevelopmentPlacement::SourceDesktop,
        )
        .unwrap();
        host.enable_timings();
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.start(executable.clone()).unwrap();
            let mut basis = prepare_playable(&mut host);
            for attempt in 0..=RETAINED {
                let speed = if class == "body" {
                    if attempt % 2 == 1 {
                        fixture.write(
                            GAMEPLAY,
                            &source_a.replacen(
                                "direction * 160.0 * delta",
                                "direction * 80.0 * delta",
                                1,
                            ),
                        );
                        80.0
                    } else {
                        fixture.write(GAMEPLAY, &source_a);
                        160.0
                    }
                } else {
                    160.0
                };
                if class == "structure" {
                    for (path, text) in &original {
                        fixture.write(path, text);
                    }
                    if attempt % 2 == 1 {
                        assert!(!fixture.edit_structure().is_empty());
                    }
                }
                let before = host.timing_snapshot().unwrap();
                let active = host.active().unwrap();
                let started = Instant::now();
                let prepare_started = Instant::now();
                let candidate = host
                    .prepare_rebuild(
                        &fixture.desktop_request(),
                        &basis,
                        &probe_steps(&basis, speed),
                    )
                    .unwrap();
                let prepare_ns = nanos(prepare_started.elapsed());
                assert_probe_was_discarded(&host, candidate.generation());
                let presentation = candidate.presentation().unwrap();
                assert!(matches!(
                    presentation.adapter_backend.as_str(),
                    "dx12" | "vulkan"
                ));
                assert_ne!(presentation.adapter_type, "cpu");
                let backend = presentation.adapter_backend.clone();
                assert_preserved(&mut host, &active, &basis);
                let adopt_started = Instant::now();
                let adopted = host.adopt(&candidate).unwrap();
                let adopt_ns = nanos(adopt_started.elapsed());
                assert_eq!(adopted.predecessor, active);
                assert_eq!(adopted.retirement, RetirementStatus::Pending);
                assert_preserved(&mut host, &adopted.active, &basis);
                host.step(Vec::new()).unwrap();
                let changed = expected_held_tick(&basis, speed);
                assert_preserved(&mut host, &adopted.active, &changed);
                assert!(
                    host.retirements()
                        .iter()
                        .all(|report| report.generation != active)
                );
                let activation_started = Instant::now();
                let activation = host.activate_active().unwrap();
                assert_eq!(
                    activation.activation,
                    InputActivation::Active,
                    "{activation:?}"
                );
                assert!(activation.error.is_none(), "{activation:?}");
                let activation_ns = nanos(activation_started.elapsed());
                let stopped = host.capture().unwrap();
                assert_eq!(stopped.player, changed.player);
                assert_eq!(stopped.held, [false; 4]);
                let running = host.resume_active().unwrap();
                assert_eq!(running.state, ExecutionState::Running);
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    let observed = host.execution_status().unwrap();
                    assert_eq!(observed.state, ExecutionState::Running);
                    assert_eq!(observed.observation.player, stopped.player.translation);
                    assert_eq!(observed.observation.held, [false; 4]);
                    assert!(observed.observation.error.is_none());
                    if observed.observation.logical_tick >= running.observation.logical_tick + 2 {
                        break;
                    }
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(10));
                }
                let correct_running_ns = nanos(started.elapsed());
                let before_retirement = host.timing_snapshot().unwrap();
                let retirement_started = Instant::now();
                finish_retirements(&mut host, adopted.retirement);
                let retirement_ns = nanos(retirement_started.elapsed());
                assert_clean_retirement(&host, active);
                let after_retirement = host.timing_snapshot().unwrap();
                samples.push(json!({
                    "class": class,
                    "attempt": attempt,
                    "warmup": attempt == 0,
                    "movement_speed": speed,
                    "backend": backend,
                    "prepared_generation": candidate.generation().sequence,
                    "captured_executable": format!("{:?}", adopted.active.executable),
                    "prepare_ns": prepare_ns,
                    "adopt_ns": adopt_ns,
                    "activation_ns": activation_ns,
                    "correct_running_ns": correct_running_ns,
                    "predecessor_retirement_ns": retirement_ns,
                    "workflow_phases": phases(before, before_retirement),
                    "retirement_phases": phases(before_retirement, after_retirement),
                }));
                basis = return_to_isolated_hold(&mut host);
                eprintln!(
                    "development_cost: class={class} attempt={attempt} correct_running_ns={correct_running_ns}"
                );
            }
        }));
        let cleanup = catch_unwind(AssertUnwindSafe(|| {
            assert!(close_to_completion(&mut host));
            for report in host.retirements() {
                assert_clean_retirement(&host, report.generation);
            }
        }));
        if let Err(cleanup) = cleanup {
            eprintln!(
                "development cost: explicit cleanup failed for {}",
                fixture.root.display()
            );
            drop(host);
            resume_unwind(cleanup);
        }
        drop(host);
        if let Err(panic) = result {
            resume_unwind(panic);
        }
    }
    drop(fixture);
    let report = json!({
        "format": 1,
        "toolchain": "1.95.0",
        "platform": format!("windows-{}", std::env::consts::ARCH),
        "outer_debug_assertions": cfg!(debug_assertions),
        "child_profile": "dev",
        "build_jobs": 1,
        "offline": true,
        "initial_build_ns": initial_build_ns,
        "retained_per_class": RETAINED,
        "warmups_per_class": 1,
        "source_write_included": false,
        "correct_running_endpoint": "complete-probe/fresh-adopter/presentation/adoption/new-logic-step/activation/Resume-and-two-naturally-completed-ticks",
        "phase_spans": "inclusive; nested phases overlap; cumulative maxima are not per-attempt maxima",
        "native_cleanup": "all original game/exit/containment/readers/output reports normal",
        "samples": samples,
    });
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
