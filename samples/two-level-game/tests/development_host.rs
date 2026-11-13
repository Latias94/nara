//! Public production Host acceptance, separate from the original process/protocol oracle.

#![cfg(any(windows, target_os = "linux"))]
#![forbid(unsafe_code)]

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::Path,
};

use nara::{
    project_tooling::cargo_build::CargoBuildFailure, project_workflow::cargo::CargoBuildError,
};
use two_level_game::{
    development::{
        DevelopmentHost, DevelopmentStage, ProbeStep, RetirementRole, RetirementStatus, protocol,
    },
    levels::{LevelId, LevelKey},
    resume::{ResumeState, encode_resume},
};

#[path = "support/development_host.rs"]
mod acceptance;
#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod support;

use acceptance::{
    WorkParent, assert_clean_retirement, assert_observation, assert_preserved,
    assert_probe_was_discarded, close_to_completion, expected_held_tick, expected_marker_tick,
    finish_retirements, movement, probe_steps, walk_to_score,
};
use protocol::SemanticCommand;
use support::{COMPILE_ERROR, Fixture, GAMEPLAY};

#[test]
fn construction_refuses_missing_project_and_non_directory_work_parent() {
    let parent = WorkParent::new();
    let missing = parent.path().join("missing-project");
    assert!(DevelopmentHost::new(&missing, parent.path()).is_err());
    assert!(
        !missing.exists(),
        "construction must not invent a source project",
    );

    let file = parent.path().join("not-a-directory");
    std::fs::write(&file, b"trusted parent must be a directory").unwrap();
    assert!(DevelopmentHost::new(Path::new(env!("CARGO_MANIFEST_DIR")), &file).is_err());
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"trusted parent must be a directory",
    );
}

#[test]
fn unstarted_and_closed_hosts_refuse_game_operations_without_launching_a_child() {
    let parent = WorkParent::new();
    let mut host =
        DevelopmentHost::new(Path::new(env!("CARGO_MANIFEST_DIR")), parent.path()).unwrap();
    assert!(host.active().is_none());
    assert!(host.last_build_failure().is_none());
    assert!(host.capture().is_err());
    assert!(host.step(vec![SemanticCommand::Marker]).is_err());
    assert!(host.active().is_none());
    assert!(matches!(
        host.retry_retirement(),
        RetirementStatus::Complete
    ));
    assert!(matches!(host.close(), RetirementStatus::Complete));
    assert!(host.active().is_none());
    assert!(host.capture().is_err());
    assert!(host.step(Vec::new()).is_err());
    assert!(matches!(host.close(), RetirementStatus::Complete));
}

#[test]
#[ignore = "actual Rust 1.95.0 nested Cargo builds and native children; run serially with jobs=1 and the absolute root CARGO_TARGET_DIR"]
fn production_host_adopts_real_body_and_structure_edits_without_probe_state_leaks() {
    assert_eq!(
        std::env::var("CARGO_BUILD_JOBS").as_deref(),
        Ok("1"),
        "the actual-build qualification shares the root target and runs one Cargo job",
    );
    let fixture = Fixture::new();
    let source_a = fixture.read(GAMEPLAY);
    let a = fixture.build();
    let executable_a = a.digest();
    let provenance_a = a.provenance().digest();
    let work_parent = fixture.directory("production-hosts");
    let mut host = DevelopmentHost::new(&fixture.project, &work_parent).unwrap();
    let mut foreign = DevelopmentHost::new(&fixture.project, &work_parent).unwrap();

    let result = catch_unwind(AssertUnwindSafe(|| {
        let ready = host.start(a.clone()).unwrap();
        assert_eq!(ready.player, [-120.0, 0.0]);
        assert_eq!(ready.held, [false; 4]);
        let generation_a = host.active().unwrap();
        assert_eq!(generation_a.executable, executable_a);
        assert_eq!(generation_a.provenance, provenance_a);
        let initial = host.capture().unwrap();
        for refusal in [
            host.execution_status().unwrap_err(),
            host.pause_active().unwrap_err(),
            host.resume_active().unwrap_err(),
        ] {
            assert_eq!(refusal.stage, DevelopmentStage::State);
        }
        assert_preserved(&mut host, &generation_a, &initial);
        let foreign_ready = foreign.start(a).unwrap();
        let foreign_generation = foreign.active().unwrap();
        let foreign_state = foreign.capture().unwrap();
        assert_observation(&foreign_ready, &foreign_state);
        assert_ne!(generation_a.pid, foreign_generation.pid);

        walk_to_score(&mut host, 10);
        host.step(vec![movement(false)]).unwrap();
        assert_eq!(
            host.step(vec![SemanticCommand::Marker]).unwrap().markers,
            [1]
        );
        host.step(vec![SemanticCommand::Travel]).unwrap();
        walk_to_score(&mut host, 20);
        let meaningful = host.step(vec![SemanticCommand::Marker]).unwrap();
        let baseline = host.capture().unwrap();
        assert_meaningful(&baseline);
        assert_observation(&meaningful, &baseline);
        let request = fixture.request();

        assert!(host.prepare_rebuild(&request, &baseline, &[]).is_err());
        let excessive = (0..65)
            .map(|_| ProbeStep {
                commands: Vec::new(),
                expected: baseline.clone(),
            })
            .collect::<Vec<_>>();
        assert!(
            host.prepare_rebuild(&request, &baseline, &excessive)
                .is_err()
        );
        assert_preserved(&mut host, &generation_a, &baseline);

        assert_eq!(baseline.player.rotation.to_bits(), 0.0_f32.to_bits());
        let mut wrong_initial = baseline.clone();
        wrong_initial.player.rotation = -0.0;
        assert_eq!(wrong_initial, baseline);
        assert_ne!(
            encode_resume(&wrong_initial).unwrap(),
            encode_resume(&baseline).unwrap(),
        );
        let error = host
            .prepare_rebuild(&request, &wrong_initial, &probe_steps(&baseline, 160.0))
            .unwrap_err();
        assert!(matches!(error.stage, DevelopmentStage::Capture), "{error}");
        assert!(host.last_build_failure().is_none());
        assert!(host.retirements().is_empty());
        assert_preserved(&mut host, &generation_a, &baseline);

        fixture.write(
            GAMEPLAY,
            &format!("{source_a}\ncompile_error!({COMPILE_ERROR:?});\n"),
        );
        assert!(
            host.prepare_rebuild(&request, &baseline, &probe_steps(&baseline, 80.0))
                .is_err()
        );
        assert!(
            matches!(host.last_build_failure(),
                Some(CargoBuildFailure::Compilation { status, diagnostics, protocol })
                    if !status.success()
                        && matches!(protocol, Some(CargoBuildError::BuildFailed))
                        && diagnostics.entries().iter().any(|entry| entry.message() == COMPILE_ERROR)),
            "the production Host must retain the typed compiler failure and its diagnostics: {:?}",
            host.last_build_failure(),
        );
        assert_preserved(&mut host, &generation_a, &baseline);
        let original_step = host.step(Vec::new()).unwrap();
        let mut base_a = expected_held_tick(&baseline, 160.0);
        assert_preserved(&mut host, &generation_a, &base_a);
        assert_observation(&original_step, &base_a);
        fixture.write(GAMEPLAY, &source_a);
        fixture.edit_body();

        let mut wrong_probe = probe_steps(&base_a, 80.0);
        wrong_probe[0].expected.counters.saves += 1;
        wrong_probe[0].expected.validate().unwrap();
        let error = host
            .prepare_rebuild(&request, &base_a, &wrong_probe)
            .unwrap_err();
        assert!(matches!(error.stage, DevelopmentStage::Probe), "{error}");
        let retirement = host.retry_retirement();
        finish_retirements(&mut host, retirement);
        let rejected_probe = host.retirements().last().unwrap();
        assert_eq!(rejected_probe.role, RetirementRole::Probe);
        assert_ne!(rejected_probe.generation.pid, generation_a.pid);
        assert_ne!(rejected_probe.generation.executable, executable_a);
        assert_ne!(rejected_probe.generation.provenance, provenance_a);
        assert_clean_retirement(&host, rejected_probe.generation);
        assert_preserved(&mut host, &generation_a, &base_a);

        let cancelled = host
            .prepare_rebuild(&request, &base_a, &probe_steps(&base_a, 80.0))
            .unwrap();
        assert_preserved(&mut host, &generation_a, &base_a);
        let continued = host.step(Vec::new()).unwrap();
        base_a = expected_held_tick(&base_a, 160.0);
        assert_observation(&continued, &base_a);
        let marked = host.step(vec![SemanticCommand::Marker]).unwrap();
        base_a = expected_marker_tick(&base_a, 160.0);
        assert_observation(&marked, &base_a);
        assert_preserved(&mut host, &generation_a, &base_a);
        let retirement = host.cancel(&cancelled).unwrap();
        finish_retirements(&mut host, retirement);
        assert_clean_retirement(&host, cancelled.generation());
        assert_preserved(&mut host, &generation_a, &base_a);
        assert!(host.adopt(&cancelled).is_err());
        assert_preserved(&mut host, &generation_a, &base_a);

        let stale = host
            .prepare_rebuild(&request, &base_a, &probe_steps(&base_a, 80.0))
            .unwrap();
        assert_preserved(&mut host, &generation_a, &base_a);
        let continued = host.step(Vec::new()).unwrap();
        base_a = expected_held_tick(&base_a, 160.0);
        assert_observation(&continued, &base_a);
        assert!(host.adopt(&stale).is_err());
        assert_preserved(&mut host, &generation_a, &base_a);
        let retirement = host.cancel(&stale).unwrap();
        finish_retirements(&mut host, retirement);
        assert_clean_retirement(&host, stale.generation());
        let continued = host.step(Vec::new()).unwrap();
        base_a = expected_held_tick(&base_a, 160.0);
        assert_preserved(&mut host, &generation_a, &base_a);
        assert_observation(&continued, &base_a);

        let drifted = host
            .prepare_rebuild(&request, &base_a, &probe_steps(&base_a, 80.0))
            .unwrap();
        assert_preserved(&mut host, &generation_a, &base_a);
        let source_b = fixture.read(GAMEPLAY);
        let body_b = "direction * 80.0 * delta";
        assert_eq!(source_b.matches(body_b).count(), 1);
        fixture.write(
            GAMEPLAY,
            &source_b.replacen(body_b, "direction * 40.0 * delta", 1),
        );
        let error = host.adopt(&drifted).unwrap_err();
        assert!(matches!(error.stage, DevelopmentStage::Adopt), "{error}");
        assert_preserved(&mut host, &generation_a, &base_a);
        let continued = host.step(Vec::new()).unwrap();
        base_a = expected_held_tick(&base_a, 160.0);
        assert_observation(&continued, &base_a);
        assert_preserved(&mut host, &generation_a, &base_a);
        let retirement = host.cancel(&drifted).unwrap();
        finish_retirements(&mut host, retirement);
        assert_clean_retirement(&host, drifted.generation());
        assert_preserved(&mut host, &generation_a, &base_a);
        fixture.write(GAMEPLAY, &source_b);
        assert_eq!(fixture.read(GAMEPLAY), source_b);

        let prepared_b = host
            .prepare_rebuild(&request, &base_a, &probe_steps(&base_a, 80.0))
            .unwrap();
        let copied_b = prepared_b.clone();
        assert_preserved(&mut host, &generation_a, &base_a);
        assert_probe_was_discarded(&host, prepared_b.generation());
        assert!(foreign.adopt(&copied_b).is_err());
        assert!(foreign.cancel(&copied_b).is_err());
        assert_preserved(&mut foreign, &foreign_generation, &foreign_state);
        assert_preserved(&mut host, &generation_a, &base_a);

        let adopted_b = host.adopt(&prepared_b).unwrap();
        assert_eq!(adopted_b.active, prepared_b.generation());
        assert_eq!(adopted_b.predecessor, generation_a);
        assert_ne!(adopted_b.active.pid, generation_a.pid);
        assert!(adopted_b.active.sequence > generation_a.sequence);
        assert_ne!(adopted_b.active.executable, executable_a);
        assert_ne!(adopted_b.active.provenance, provenance_a);
        assert!(matches!(adopted_b.retirement, RetirementStatus::Pending));
        assert_preserved(&mut host, &adopted_b.active, &base_a);
        let error = host
            .prepare_rebuild(&request, &base_a, &probe_steps(&base_a, 80.0))
            .unwrap_err();
        assert!(matches!(error.stage, DevelopmentStage::State), "{error}");
        assert_preserved(&mut host, &adopted_b.active, &base_a);
        let body_step = host.step(Vec::new()).unwrap();
        let base_b = expected_held_tick(&base_a, 80.0);
        assert_observation(&body_step, &base_b);
        assert_preserved(&mut host, &adopted_b.active, &base_b);
        assert!(
            host.retirements()
                .iter()
                .all(|report| report.generation != generation_a),
            "the predecessor must remain owned until retirement is explicitly driven",
        );
        assert!(
            body_step.engine_tick < original_step.engine_tick,
            "the adopter must have its own fresh Runtime clock",
        );
        finish_retirements(&mut host, adopted_b.retirement);
        assert_clean_retirement(&host, generation_a);
        assert_preserved(&mut host, &adopted_b.active, &base_b);
        assert!(host.adopt(&copied_b).is_err());
        assert_preserved(&mut host, &adopted_b.active, &base_b);

        let structural_inputs = fixture.edit_structure();
        assert!(structural_inputs.contains(GAMEPLAY));
        let prepared_c = host
            .prepare_rebuild(&request, &base_b, &probe_steps(&base_b, 80.0))
            .unwrap();
        assert_preserved(&mut host, &adopted_b.active, &base_b);
        assert_probe_was_discarded(&host, prepared_c.generation());
        assert!(host.adopt(&copied_b).is_err());
        assert_preserved(&mut host, &adopted_b.active, &base_b);
        let adopted_c = host.adopt(&prepared_c).unwrap();
        assert_eq!(adopted_c.active, prepared_c.generation());
        assert_eq!(adopted_c.predecessor, adopted_b.active);
        assert_ne!(adopted_c.active.pid, adopted_b.active.pid);
        assert!(adopted_c.active.sequence > adopted_b.active.sequence);
        assert_ne!(adopted_c.active.executable, adopted_b.active.executable);
        assert_ne!(adopted_c.active.provenance, adopted_b.active.provenance);
        assert!(matches!(adopted_c.retirement, RetirementStatus::Pending));
        assert_preserved(&mut host, &adopted_c.active, &base_b);
        assert!(host.adopt(&prepared_c).is_err());
        assert_preserved(&mut host, &adopted_c.active, &base_b);

        for probe in probe_steps(&base_b, 80.0) {
            let observed = host.step(probe.commands).unwrap();
            assert_observation(&observed, &probe.expected);
            assert_preserved(&mut host, &adopted_c.active, &probe.expected);
        }
        assert!(
            host.retirements()
                .iter()
                .all(|report| report.generation != adopted_b.active),
            "Host Close must retain and retire both the predecessor and the adopted active game",
        );
        assert_preserved(&mut foreign, &foreign_generation, &foreign_state);
    }));

    let closed = close_to_completion(&mut host);
    let foreign_closed = close_to_completion(&mut foreign);
    if !closed || !foreign_closed {
        let retained_tree = fixture.root.clone();
        std::mem::forget(host);
        std::mem::forget(foreign);
        std::mem::forget(fixture);
        panic!(
            "physical Host cleanup remains pending; retained owners and their tree at {}",
            retained_tree.display(),
        );
    }
    for retired in host.retirements() {
        assert_clean_retirement(&host, retired.generation);
    }
    for retired in foreign.retirements() {
        assert_clean_retirement(&foreign, retired.generation);
    }
    assert!(host.active().is_none());
    assert!(foreign.active().is_none());
    if let Err(error) = result {
        resume_unwind(error);
    }
}

fn assert_meaningful(state: &ResumeState) {
    state.validate().unwrap();
    assert_eq!(
        state.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_ne!(state.player.translation, [-120.0, 0.0]);
    assert_eq!(state.held, [false, true, false, false]);
    assert_eq!(state.progress.score, 20);
    assert_eq!(state.progress.collected.len(), 2);
    for level in [LevelId::Meadow, LevelId::Cavern] {
        let key = LevelKey::main(level);
        assert!(
            state
                .progress
                .collected
                .iter()
                .any(|pickup| pickup.level == key)
        );
        assert!(
            state
                .progress
                .markers
                .iter()
                .any(|marker| marker.level == key)
        );
    }
    assert_eq!(state.progress.markers.len(), 2);
    assert_eq!(state.progress.next_marker_id.get(), 3);
    assert_eq!(state.compatibility.levels.len(), 2);
    assert_eq!(state.counters.travel_count, 1);
    assert!(state.clock.ticks > 0);
    assert_eq!(state.clock.timestep_nanos, 20_000_000);
}
