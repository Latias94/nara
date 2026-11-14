#![cfg(all(feature = "desktop", windows))]
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
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{fs::ContentDigest, project_tooling::cargo_build::build_executable};
use serde_json::json;
use two_level_game::{
    development::{
        DevelopmentHost, DevelopmentPlacement, DevelopmentStage, DevelopmentTimingPhase as Phase,
        ExecutionState, InputActivation, RetirementRole, RetirementStatus, protocol,
    },
    levels::{LevelId, LevelKey},
    resume::{SessionCounters, encode_resume},
    save::Checkpoint,
};

use acceptance::{assert_clean_retirement, assert_observation, close_to_completion};
use support::Fixture;

const RETAINED: usize = 5;
const CHILD_PHASES: [Phase; 4] = [
    Phase::ChildNamespacePreparation,
    Phase::ChildLaunchAuthority,
    Phase::ChildNativeSpawn,
    Phase::ChildReadyWait,
];

fn nanos(duration: Duration) -> u64 {
    duration.as_nanos().try_into().unwrap()
}

fn timestamp() -> u64 {
    nanos(SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
}

#[test]
#[ignore = "actual nested builds and Windows hardware desktop; run serially with an explicit new absolute NARA_DEVELOPMENT_STARTUP_OUTPUT JSON path"]
fn production_hosts_record_independent_initial_startup() {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let output = PathBuf::from(
        std::env::var_os("NARA_DEVELOPMENT_STARTUP_OUTPUT")
            .expect("supply an explicit new absolute JSON result path"),
    );
    assert!(output.is_absolute() && !output.exists());
    assert!(output.parent().unwrap().is_dir());
    let fixture = Fixture::new();
    let head = Command::new("git")
        .current_dir(&fixture.repository)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    let variants = [
        (DevelopmentPlacement::SourceHeadless, fixture.request()),
        (
            DevelopmentPlacement::SourceDesktop,
            fixture.desktop_request(),
        ),
    ]
    .map(|(placement, request)| {
        let started_unix_ns = timestamp();
        let started = Instant::now();
        let executable = build_executable(&request, |_| Ok(())).unwrap();
        let build_wall_ns = nanos(started.elapsed());
        let provenance = executable.provenance();
        assert_eq!(provenance.selection(), request.selection());
        assert_eq!(
            provenance.resolved_features(),
            request.selection().features()
        );
        assert_eq!(
            executable.digest(),
            ContentDigest::of_bytes(executable.bytes())
        );
        let build = json!({
            "placement": placement, "started_unix_ns": started_unix_ns,
            "finished_unix_ns": timestamp(), "build_wall_ns": build_wall_ns,
            "declared_child_inputs": provenance.declared_inputs(),
            "provenance_digest": format!("{:?}", provenance.digest()),
            "metadata_digest": format!("{:?}", provenance.metadata_digest()),
            "executable_digest": format!("{:?}", executable.digest()),
            "compiler_and_tools": provenance.toolchain(),
            "fixture_verified_rustc_release": "1.95.0",
            "child_profile": provenance.selection().profile(),
            "target": provenance.selection().target_triple(),
            "requested_features": provenance.selection().features(),
            "resolved_child_features": provenance.resolved_features(),
            "child_default_features": provenance.selection().default_features(),
        });
        (placement, executable, build)
    });
    let manifest = fixture.read("nara.toml");
    let mut canonical = [None, None];
    let mut samples = Vec::with_capacity((RETAINED + 1) * variants.len());
    let mut bootstrap_failure = None;
    for round in 0..=RETAINED + 1 {
        let failing = round == RETAINED + 1;
        let placements = if failing { 1 } else { variants.len() };
        for position in 0..placements {
            let index = if failing {
                0
            } else {
                (round + position) % variants.len()
            };
            let (placement, executable, _) = &variants[index];
            let parent = fixture.directory(&format!("startup-{round}-{position}"));
            if failing {
                fixture.write("nara.toml", "[project\n");
            }
            let mut host =
                DevelopmentHost::with_placement(&fixture.project, &parent, *placement).unwrap();
            host.enable_timings();
            let result = catch_unwind(AssertUnwindSafe(|| {
                let captured = executable.clone();
                let started_unix_ns = timestamp();
                let started = Instant::now();
                let ready = host.start(captured);
                let elapsed = started.elapsed();
                let snapshot = host.timing_snapshot().unwrap();
                let finished_unix_ns = timestamp();
                let phases: Vec<_> = Phase::ALL
                    .into_iter()
                    .map(|phase| {
                        let timing = snapshot.get(phase);
                        let expected = u64::from(
                            matches!(phase, Phase::ExecutableInstall | Phase::ChildStartup)
                                || CHILD_PHASES.contains(&phase),
                        );
                        let failures = u64::from(
                            failing && matches!(phase, Phase::ChildStartup | Phase::ChildReadyWait),
                        );
                        if !(failing && phase == Phase::Retirement) {
                            assert_eq!(
                                (timing.count, timing.failures),
                                (expected, failures),
                                "{phase:?}"
                            );
                            assert_eq!(timing.max_elapsed, timing.total_elapsed, "{phase:?}");
                            if expected == 0 {
                                assert_eq!(timing.total_elapsed, Duration::ZERO, "{phase:?}");
                            }
                        }
                        json!({
                            "phase": format!("{phase:?}"), "operations": timing.count,
                            "failures": timing.failures,
                            "inclusive_elapsed_ns": nanos(timing.total_elapsed),
                            "maximum_ns": nanos(timing.max_elapsed),
                        })
                    })
                    .collect();
                let startup = snapshot.get(Phase::ChildStartup).total_elapsed;
                let children: Duration = CHILD_PHASES
                    .into_iter()
                    .map(|phase| snapshot.get(phase).total_elapsed)
                    .sum();
                assert!(children <= startup);
                assert!(snapshot.get(Phase::ExecutableInstall).total_elapsed + startup <= elapsed);
                if failing {
                    let error = ready.unwrap_err();
                    assert_eq!(error.stage, DevelopmentStage::Start);
                    assert!(host.active().is_none());
                    return json!({
                        "scenario": "invalid_project_toml_after_native_spawn", "placement": placement,
                        "started_unix_ns": started_unix_ns, "finished_unix_ns": finished_unix_ns,
                        "error": error.to_string(), "phases_at_start_return": phases,
                    });
                }
                let ready = ready.unwrap();
                let active = host.active().unwrap();
                assert_eq!(active.sequence, 1);
                assert_ne!(active.pid, 0);
                assert_eq!(active.executable, executable.digest());
                assert_eq!(active.provenance, executable.provenance().digest());
                let state = host.capture().unwrap();
                assert_observation(&ready, &state);
                assert_ne!(ready.runtime_generation, 0);
                assert_eq!(ready.engine_tick, 0);
                assert_eq!(state.player.translation, [-120.0, 0.0]);
                assert_eq!(state.player.rotation, 0.0);
                assert_eq!(state.player.scale, [1.0; 2]);
                assert_eq!(
                    state.progress.location.level,
                    LevelKey::main(LevelId::STARTUP)
                );
                assert_eq!(state.progress.location.anchor.as_str(), "checkpoint");
                assert_eq!(
                    state.progress,
                    Checkpoint::new(state.progress.location.clone())
                );
                assert_eq!(state.counters, SessionCounters::default());
                assert_eq!(state.held, [false; 4]);
                assert_eq!((state.clock.ticks, state.clock.elapsed_nanos), (0, 0));
                assert_eq!(state.runtime_time.pending_nanos, 0);
                let bytes = encode_resume(&state).unwrap();
                let digest = *ContentDigest::of_bytes(&bytes).as_bytes();
                match &canonical[index] {
                    Some(expected) => assert_eq!(&bytes, expected),
                    None => canonical[index] = Some(bytes.clone()),
                }
                let desktop = (*placement == DevelopmentPlacement::SourceDesktop).then(|| {
                    let status = host.execution_status().unwrap();
                    assert_eq!(status.state, ExecutionState::Paused);
                    assert_eq!(status.input_activation, InputActivation::Isolated);
                    assert_eq!(status.observation, ready);
                    let receipt = host.confirm_active_presentation().unwrap();
                    assert_eq!(receipt.runtime_generation, ready.runtime_generation);
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
                    assert_eq!(host.execution_status().unwrap(), status);
                    json!({ "status": status, "presentation": receipt })
                });
                assert_eq!(encode_resume(&host.capture().unwrap()).unwrap(), bytes);
                assert_eq!(host.active(), Some(active));
                json!({
                    "round": round, "position": position, "placement": placement,
                    "excluded_warmup": round == 0,
                    "started_unix_ns": started_unix_ns, "finished_unix_ns": finished_unix_ns,
                    "host_start_wall_ns": nanos(elapsed), "phases_at_start_return": phases,
                    "active": { "sequence": active.sequence, "pid": active.pid,
                        "executable_digest": format!("{:?}", active.executable),
                        "provenance_digest": format!("{:?}", active.provenance) },
                    "ready": ready, "state_digest": digest, "desktop": desktop,
                })
            }));
            let active_at_cleanup = host.active();
            let cleanup = catch_unwind(AssertUnwindSafe(|| {
                assert!(close_to_completion(&mut host));
                assert!(host.cleanup_error().is_none());
                assert!(host.active().is_none() && host.last_build_failure().is_none());
                assert_eq!(host.retry_retirement(), RetirementStatus::Complete);
                assert_eq!(host.retirements().len(), 1);
                let report = &host.retirements()[0];
                if failing {
                    assert_eq!(report.role, RetirementRole::Candidate);
                    assert_eq!(report.generation.sequence, 1);
                    assert_ne!(report.generation.pid, 0);
                    assert_eq!(report.generation.executable, executable.digest());
                    assert_eq!(
                        report.generation.provenance,
                        executable.provenance().digest()
                    );
                    assert!(report.native.containment_empty());
                    assert!(report.native.readers_stopped() && report.native.output_drained());
                } else {
                    assert_eq!(Some(report.generation), active_at_cleanup);
                    assert_clean_retirement(&host, report.generation);
                }
                if failing {
                    fixture.write("nara.toml", &manifest);
                    assert_eq!(fixture.read("nara.toml"), manifest);
                }
                json!({
                    "sequence": report.generation.sequence, "pid": report.generation.pid,
                    "role": format!("{:?}", report.role),
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
                    let root = fixture.root.clone();
                    std::mem::forget((host, fixture));
                    eprintln!(
                        "development startup retains original owners at {}",
                        root.display()
                    );
                    resume_unwind(panic);
                }
            };
            drop(host);
            let mut sample = match result {
                Ok(sample) => sample,
                Err(panic) => {
                    drop(fixture);
                    resume_unwind(panic);
                }
            };
            sample["cleanup"] = cleanup;
            if failing {
                sample["original_manifest_restored"] = json!(true);
                bootstrap_failure = Some(sample);
            } else {
                samples.push(sample);
            }
        }
    }
    let fixture_root = fixture.root.clone();
    drop(fixture);
    assert!(
        !fixture_root.exists(),
        "source fixture cleanup must finish before output"
    );
    let builds = variants.map(|(_, _, build)| build);
    let canonical = canonical.map(|bytes| String::from_utf8(bytes.unwrap()).unwrap());
    let report = json!({
        "format": 1, "platform": format!("windows-{}", std::env::consts::ARCH),
        "repository_head_at_setup": String::from_utf8(head.stdout).unwrap().trim(),
        "measurement_source_digest": format!("{:?}", ContentDigest::of_bytes(include_bytes!("development_startup.rs"))),
        "outer_debug_assertions": cfg!(debug_assertions),
        "outer_features": { "desktop": cfg!(feature = "desktop"),
            "cost-measurement": cfg!(feature = "cost-measurement") },
        "rustup_toolchain": std::env::var("RUSTUP_TOOLCHAIN").unwrap(),
        "build_jobs": 1, "offline": true, "builds": builds,
        "canonical_initialized_resume_utf8": { "source_headless": canonical[0], "source_desktop": canonical[1] },
        "retained_per_placement": RETAINED, "excluded_warmups_per_placement": 1,
        "order": "round r, position p: [source_headless, source_desktop][(r + p) % 2]; round zero is excluded; every entry owns a fresh independent Host and original child",
        "timing_scope": "Instant around complete host.start Result, including executable installation, ChildStartup and the immediate post-READY observation; builds, caller assertions, capture, presentation and cleanup are excluded; Unix timestamps bracket the call and immediate timing snapshot",
        "phase_scope": "snapshot immediately after start; successful performance samples have one operation and zero failures for install, ChildStartup and each subphase, with other phases zero; excluded bootstrap failure separately has ChildStartup and ChildReadyWait failures; four subphases sum to at most ChildStartup with small unmeasured construction gaps; neither an exact partition nor a complete host.start total; parent and children overlap",
        "validation_scope": "fresh initialized paused game, original generation and executable/provenance, neutral held input and zero ticks; canonical bytes match only within each placement; desktop additionally confirms fresh original non-CPU presentation with matching digest and nonzero IDs; recipes/features intentionally differ",
        "source_scope": "declared fixture inputs and captured executables; setup HEAD does not snapshot shared root dependencies; compiler/tool digests and child profiles/features are observed separately from outer debug_assertions",
        "source_fixture_removed": true, "samples": samples,
        "excluded_bootstrap_failure_acceptance": bootstrap_failure.unwrap(),
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
