//! Finite paused control turnaround observations through the production desktop Host.

#![cfg(all(feature = "desktop", windows, feature = "development"))]
#![forbid(unsafe_code)]

#[path = "support/development_host.rs"]
#[allow(dead_code)]
mod acceptance;
#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod support;

use std::{
    fs::OpenOptions,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
    process::Command,
    time::Instant,
};

use nara::{fs::ContentDigest, project_tooling::cargo_build::build_executable};
use serde_json::json;
use two_level_game::{
    development::{
        DevelopmentHost, DevelopmentPlacement, ExecutionState, InputActivation, protocol,
    },
    levels::{LevelId, LevelKey},
    resume::{ResumeState, encode_resume},
};

use acceptance::{
    assert_clean_retirement, assert_observation, close_to_completion, movement, walk_to_score,
};
use support::Fixture;

const RETAINED: usize = 10;
const OPERATIONS: [&str; 3] = ["capture", "execution_status", "confirm_active_presentation"];

fn prepare_basis(host: &mut DevelopmentHost) -> ResumeState {
    host.isolate_active_input().unwrap();
    walk_to_score(host, 10);
    host.step(vec![movement(false)]).unwrap();
    host.step(vec![protocol::SemanticCommand::Marker]).unwrap();
    host.step(vec![protocol::SemanticCommand::Travel]).unwrap();
    walk_to_score(host, 20);
    host.step(vec![movement(false)]).unwrap();
    host.step(vec![protocol::SemanticCommand::Marker]).unwrap();
    let basis = host.capture().unwrap();
    assert_eq!(
        basis.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(basis.progress.score, 20);
    assert_eq!(basis.progress.collected.len(), 2);
    assert_eq!(basis.progress.markers.len(), 2);
    for level in [LevelId::Meadow, LevelId::Cavern] {
        assert!(
            basis
                .progress
                .markers
                .iter()
                .any(|marker| marker.level == LevelKey::main(level))
        );
    }
    assert_eq!(basis.counters.travel_count, 1);
    assert_eq!(basis.held, [false; 4]);
    assert!(basis.clock.ticks > 0);
    basis
}

#[test]
#[ignore = "actual nested build and Windows hardware desktop; run serially with an explicit new absolute NARA_DEVELOPMENT_TURNAROUND_OUTPUT JSON path"]
fn production_desktop_records_paused_control_turnaround() {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let output = PathBuf::from(
        std::env::var_os("NARA_DEVELOPMENT_TURNAROUND_OUTPUT")
            .expect("supply an explicit new absolute JSON result path"),
    );
    assert!(output.is_absolute() && !output.exists());
    assert!(output.parent().unwrap().is_dir());
    let fixture = Fixture::new();
    let git = |args: &[&str]| {
        let result = Command::new("git")
            .current_dir(&fixture.repository)
            .args(args)
            .output()
            .unwrap();
        assert!(result.status.success());
        String::from_utf8(result.stdout).unwrap()
    };
    let request = fixture.desktop_request();
    let executable = build_executable(&request, |_| Ok(())).unwrap();
    let provenance = executable.provenance();
    assert_eq!(provenance.selection(), request.selection());
    let build = json!({
        "repository_head_at_setup": git(&["rev-parse", "HEAD"]).trim(),
        "repository_status_at_setup": git(&["status", "--short", "--untracked-files=normal"]),
        "measurement_source_digest": format!("{:?}", ContentDigest::of_bytes(include_bytes!("development_turnaround.rs"))),
        "declared_child_inputs": provenance.declared_inputs(),
        "provenance_digest": format!("{:?}", provenance.digest()),
        "executable_digest": format!("{:?}", executable.digest()),
        "compiler": provenance.toolchain(),
        "fixture_verified_rustc_release": "1.95.0",
        "rustup_toolchain": std::env::var("RUSTUP_TOOLCHAIN").unwrap(),
        "child_profile": provenance.selection().profile(), "target": provenance.selection().target_triple(),
        "requested_features": provenance.selection().features(),
        "resolved_child_features": provenance.resolved_features(),
        "child_default_features": provenance.selection().default_features(),
        "outer_debug_assertions": cfg!(debug_assertions),
        "source_scope": "declared fixture inputs and captured executable; repository HEAD/status are setup observations, not a snapshot of the shared root dependency",
    });
    let parent = fixture.directory("development-turnaround");
    let mut host = DevelopmentHost::with_placement(
        &fixture.project,
        &parent,
        DevelopmentPlacement::SourceDesktop,
    )
    .unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        host.start(executable).unwrap();
        let active = host.active().unwrap();
        let basis = prepare_basis(&mut host);
        let canonical = encode_resume(&basis).unwrap();
        let digest = *ContentDigest::of_bytes(&canonical).as_bytes();
        let status = host.execution_status().unwrap();
        assert_eq!(status.state, ExecutionState::Paused);
        assert_eq!(status.input_activation, InputActivation::Isolated);
        assert_observation(&status.observation, &basis);
        assert_ne!(status.observation.runtime_generation, 0);
        let mut warmups = Vec::with_capacity(OPERATIONS.len());
        let mut samples = Vec::with_capacity(RETAINED * OPERATIONS.len());
        for round in 0..=RETAINED {
            for position in 0..OPERATIONS.len() {
                let operation = OPERATIONS[(round + position) % OPERATIONS.len()];
                assert_eq!(host.active(), Some(active));
                let (elapsed, returned) = match operation {
                    "capture" => {
                        let started = Instant::now();
                        let captured = host.capture();
                        let elapsed = started.elapsed();
                        let captured = captured.unwrap();
                        assert_eq!(captured.clock, basis.clock);
                        let bytes = encode_resume(&captured).unwrap();
                        assert_eq!(bytes, canonical);
                        let actual_digest = *ContentDigest::of_bytes(&bytes).as_bytes();
                        (
                            elapsed,
                            json!({ "clock": captured.clock, "state_digest": actual_digest }),
                        )
                    }
                    "execution_status" => {
                        let started = Instant::now();
                        let observed = host.execution_status();
                        let elapsed = started.elapsed();
                        let observed = observed.unwrap();
                        assert_eq!(observed, status);
                        assert_observation(&observed.observation, &basis);
                        (elapsed, json!({ "status": observed }))
                    }
                    "confirm_active_presentation" => {
                        let started = Instant::now();
                        let receipt = host.confirm_active_presentation();
                        let elapsed = started.elapsed();
                        let receipt = receipt.unwrap();
                        assert_eq!(
                            receipt.runtime_generation,
                            status.observation.runtime_generation
                        );
                        assert_eq!(receipt.state_digest, digest);
                        assert!(
                            [
                                receipt.runtime_generation,
                                receipt.app_frame,
                                receipt.render_frame,
                                receipt.window,
                                receipt.surface_generation,
                                receipt.backend_instance,
                                receipt.device_epoch,
                            ]
                            .into_iter()
                            .all(|id| id > 0),
                            "{receipt:?}"
                        );
                        assert!(matches!(
                            receipt.adapter_backend.as_str(),
                            "dx12" | "vulkan"
                        ));
                        assert!(!receipt.adapter_type.is_empty());
                        assert_ne!(receipt.adapter_type, "cpu");
                        (elapsed, json!({ "presentation": receipt }))
                    }
                    _ => unreachable!("the operation inventory is fixed"),
                };
                assert_eq!(host.active(), Some(active));
                let sample = json!({
                    "round": round, "position": position, "operation": operation,
                    "api_wall_ns": u64::try_from(elapsed.as_nanos()).unwrap(),
                    "returned": returned,
                });
                if round == 0 {
                    warmups.push(sample);
                } else {
                    samples.push(sample);
                }
            }
        }
        assert_eq!(host.active(), Some(active));
        assert_eq!(encode_resume(&host.capture().unwrap()).unwrap(), canonical);
        assert_eq!(host.execution_status().unwrap(), status);
        json!({
            "active": { "sequence": active.sequence, "pid": active.pid,
                "executable_digest": format!("{:?}", active.executable),
                "provenance_digest": format!("{:?}", active.provenance) },
            "canonical_resume_utf8": String::from_utf8(canonical).unwrap(),
            "state_digest": digest,
            "original_execution_status": status,
            "excluded_warmups": warmups,
            "samples": samples,
        })
    }));
    let active_at_cleanup = host.active();
    let cleanup = catch_unwind(AssertUnwindSafe(|| {
        assert!(close_to_completion(&mut host));
        assert!(host.cleanup_error().is_none());
        assert_eq!(host.retirements().len(), 1);
        let report = &host.retirements()[0];
        assert_eq!(Some(report.generation), active_at_cleanup);
        assert_clean_retirement(&host, report.generation);
        json!({
            "sequence": report.generation.sequence, "pid": report.generation.pid,
            "executable_digest": format!("{:?}", report.generation.executable),
            "provenance_digest": format!("{:?}", report.generation.provenance),
            "game_closed": report.game_closed, "terminated": report.terminated,
            "exit_success": report.native.exit().success(),
            "containment_empty": report.native.containment_empty(),
            "readers_stopped": report.native.readers_stopped(),
            "output_drained": report.native.output_drained(),
        })
    }));
    let cleanup = match cleanup {
        Ok(cleanup) => cleanup,
        Err(panic) => {
            eprintln!(
                "development turnaround: explicit cleanup failed for {}",
                fixture.root.display()
            );
            drop(host);
            resume_unwind(panic);
        }
    };
    drop(host);
    let fixture_root = fixture.root.clone();
    drop(fixture);
    assert!(
        !fixture_root.exists(),
        "source fixture cleanup must finish before output"
    );
    let measurements = match result {
        Ok(measurements) => measurements,
        Err(panic) => resume_unwind(panic),
    };
    let report = json!({
        "format": 1, "platform": format!("windows-{}", std::env::consts::ARCH),
        "placement": "source_desktop", "build": build,
        "outer_features": { "desktop": cfg!(feature = "desktop"),
            "cost-measurement": cfg!(feature = "cost-measurement") },
        "build_jobs": 1, "offline": true,
        "operations": OPERATIONS, "retained_per_operation": RETAINED, "excluded_warmups_per_operation": 1,
        "operation_order": "round r, position p: operations[(r + p) % 3]; round zero is excluded",
        "timing_scope": "Instant immediately before each complete public DevelopmentHost API call through its returned Result; API-internal validation, encoding, file transport and native waits are included; caller-side validation/encoding, JSON assembly, build, startup, gameplay preparation and cleanup are excluded",
        "presentation_scope": "complete confirm_active_presentation API including its internal capture, original native presentation confirmation and subsequent observation; returned IDs and canonical state digest are original receipts, not inferred pixels or scanout",
        "execution_scope": "one hidden paused child with neutral held intent and isolated native input; the caller issues no activation, execution control, Step, restore or gameplay mutation during sampling; presentation retains its original maintenance frames",
        "validation_scope": "every measured capture equals the original encode_resume bytes and clock; every measured status equals the complete original status; every presentation has the original Runtime generation, expected canonical digest and all nonzero original IDs; final capture/status remain exact",
        "cleanup": cleanup, "source_fixture_removed": true, "measurements": measurements,
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &report).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
