//! Actual body/private-structure edits adopted through the production desktop development Host.

#![cfg(all(feature = "desktop", windows))]
#![forbid(unsafe_code)]

#[path = "support/development_host.rs"]
#[allow(dead_code)]
mod acceptance;
#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod support;

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use nara::{
    fs::ContentDigest,
    project_tooling::cargo_build::{CargoBuildFailure, build_executable},
};
use two_level_game::{
    development::{
        DevelopmentHost, DevelopmentPlacement, InputActivation, RetirementStatus,
        protocol::{self, SemanticCommand},
    },
    levels::{LevelId, LevelKey},
    resume::{ResumeState, encode_resume},
};

use acceptance::{
    assert_clean_retirement, assert_preserved, assert_probe_was_discarded, close_to_completion,
    expected_held_tick, finish_retirements, movement, probe_steps, walk_to_score,
};
use support::{COMPILE_ERROR, Fixture, GAMEPLAY};

fn check_presented(host: &mut DevelopmentHost, expected: &ResumeState) -> String {
    let presentation = host.confirm_active_presentation().unwrap();
    assert_eq!(
        presentation.state_digest,
        *ContentDigest::of_bytes(&encode_resume(expected).unwrap()).as_bytes()
    );
    assert!(presentation.surface_generation > 0 && presentation.backend_instance > 0);
    assert!(presentation.device_epoch > 0 && presentation.render_frame > 0);
    assert!(matches!(
        presentation.adapter_backend.as_str(),
        "dx12" | "vulkan"
    ));
    assert_ne!(presentation.adapter_type, "cpu");
    presentation.adapter_backend
}

#[test]
#[ignore = "actual nested Cargo builds and Windows hardware desktop; run serially with the root target"]
fn production_desktop_host_adopts_real_edits_after_exact_restored_presentation() {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let fixture = Fixture::new();
    let source_a = fixture.read(GAMEPLAY);
    let request = fixture.desktop_request();
    let a = build_executable(&request, |_| Ok(())).unwrap();
    let parent = fixture.directory("desktop-production-hosts");
    let mut host = DevelopmentHost::with_placement(
        &fixture.project,
        &parent,
        DevelopmentPlacement::SourceDesktop,
    )
    .unwrap();
    let mut foreign = DevelopmentHost::with_placement(
        &fixture.project,
        &parent,
        DevelopmentPlacement::SourceDesktop,
    )
    .unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        host.start(a).unwrap();
        let a = host.active().unwrap();
        let initial = host.capture().unwrap();
        let backend = check_presented(&mut host, &initial);
        let initial_activation = host.activate_active().unwrap();
        assert_eq!(
            initial_activation.activation,
            InputActivation::Active,
            "{initial_activation:?}"
        );
        assert!(initial_activation.error.is_none());
        host.isolate_active_input().unwrap();
        assert_preserved(&mut host, &a, &initial);
        walk_to_score(&mut host, 10);
        host.step(vec![movement(false)]).unwrap();
        host.step(vec![SemanticCommand::Marker]).unwrap();
        host.step(vec![SemanticCommand::Travel]).unwrap();
        walk_to_score(&mut host, 20);
        host.step(vec![SemanticCommand::Marker]).unwrap();
        let basis = host.capture().unwrap();
        assert_eq!(
            basis.progress.location.level,
            LevelKey::main(LevelId::Cavern)
        );
        assert_eq!(basis.progress.score, 20);
        assert_eq!(basis.progress.markers.len(), 2);
        assert_eq!(basis.held, [false, true, false, false]);
        assert_ne!(basis.player.translation, [-120.0, 0.0]);
        assert_eq!(check_presented(&mut host, &basis), backend);
        fixture.write(
            GAMEPLAY,
            &format!("{source_a}\ncompile_error!({COMPILE_ERROR:?});\n"),
        );
        assert!(
            host.prepare_rebuild(&request, &basis, &probe_steps(&basis, 80.0))
                .is_err()
        );
        assert!(matches!(
            host.last_build_failure(),
            Some(CargoBuildFailure::Compilation { .. })
        ));
        assert_preserved(&mut host, &a, &basis);
        fixture.write(GAMEPLAY, &source_a);
        fixture.edit_body();
        let mut wrong = probe_steps(&basis, 80.0);
        wrong[0].expected.counters.saves += 1;
        assert!(host.prepare_rebuild(&request, &basis, &wrong).is_err());
        let status = host.retry_retirement();
        finish_retirements(&mut host, status);
        assert_preserved(&mut host, &a, &basis);

        let cancelled = host
            .prepare_rebuild(&request, &basis, &probe_steps(&basis, 80.0))
            .unwrap();
        assert!(cancelled.presentation().is_some());
        host.step(Vec::new()).unwrap();
        let advanced_a = expected_held_tick(&basis, 160.0);
        assert!(host.adopt(&cancelled).is_err());
        let status = host.cancel(&cancelled).unwrap();
        finish_retirements(&mut host, status);
        assert_clean_retirement(&host, cancelled.generation());
        assert_preserved(&mut host, &a, &advanced_a);

        let drifted = host
            .prepare_rebuild(&request, &advanced_a, &probe_steps(&advanced_a, 80.0))
            .unwrap();
        let source_b = fixture.read(GAMEPLAY);
        fixture.write(
            GAMEPLAY,
            &source_b.replacen("direction * 80.0 * delta", "direction * 40.0 * delta", 1),
        );
        assert!(host.adopt(&drifted).is_err());
        assert_preserved(&mut host, &a, &advanced_a);
        let status = host.cancel(&drifted).unwrap();
        finish_retirements(&mut host, status);
        assert_clean_retirement(&host, drifted.generation());
        fixture.write(GAMEPLAY, &source_b);

        let prepared_b = host
            .prepare_rebuild(&request, &advanced_a, &probe_steps(&advanced_a, 80.0))
            .unwrap();
        assert_probe_was_discarded(&host, prepared_b.generation());
        assert!(foreign.adopt(&prepared_b).is_err());
        assert!(foreign.cancel(&prepared_b).is_err());
        let adopted_b = host.adopt(&prepared_b).unwrap();
        assert_eq!(adopted_b.predecessor, a);
        assert_ne!(adopted_b.active.pid, a.pid);
        assert_ne!(adopted_b.active.executable, a.executable);
        assert_eq!(adopted_b.retirement, RetirementStatus::Pending);
        assert_preserved(&mut host, &adopted_b.active, &advanced_a);
        assert!(
            host.prepare_rebuild(&request, &advanced_a, &probe_steps(&advanced_a, 80.0))
                .is_err()
        );
        host.step(Vec::new()).unwrap();
        let base_b = expected_held_tick(&advanced_a, 80.0);
        assert_preserved(&mut host, &adopted_b.active, &base_b);
        assert!(
            host.retirements()
                .iter()
                .all(|retired| retired.generation != a)
        );
        assert_eq!(check_presented(&mut host, &base_b), backend);
        finish_retirements(&mut host, adopted_b.retirement);
        assert_clean_retirement(&host, a);
        assert!(host.adopt(&prepared_b).is_err());

        fixture.edit_structure();
        let prepared_c = host
            .prepare_rebuild(&request, &base_b, &probe_steps(&base_b, 80.0))
            .unwrap();
        assert_probe_was_discarded(&host, prepared_c.generation());
        let adopted_c = host.adopt(&prepared_c).unwrap();
        assert_eq!(adopted_c.predecessor, adopted_b.active);
        assert_ne!(adopted_c.active.executable, adopted_b.active.executable);
        assert_eq!(adopted_c.retirement, RetirementStatus::Pending);
        assert_preserved(&mut host, &adopted_c.active, &base_b);
        assert_eq!(check_presented(&mut host, &base_b), backend);
        let activation = host.activate_active().unwrap();
        assert_eq!(activation.active, adopted_c.active);
        assert_eq!(
            activation.activation,
            InputActivation::Active,
            "{activation:?}"
        );
        assert!(activation.error.is_none(), "{activation:?}");
        let activated = host.capture().unwrap();
        assert_eq!(activated.player, base_b.player);
        assert_eq!(activated.held, [false; 4]);
        assert_eq!(activated.clock.ticks, base_b.clock.ticks + 1);
        finish_retirements(&mut host, adopted_c.retirement);
        assert_clean_retirement(&host, adopted_b.active);
    }));
    let closed = close_to_completion(&mut host);
    let foreign_closed = close_to_completion(&mut foreign);
    if !closed || !foreign_closed {
        let root = fixture.root.clone();
        std::mem::forget((host, foreign, fixture));
        panic!(
            "desktop development retains incomplete original cleanup at {}",
            root.display()
        );
    }
    for report in host.retirements() {
        assert_clean_retirement(&host, report.generation);
    }
    drop(host);
    drop(foreign);
    if let Err(panic) = result {
        resume_unwind(panic);
    }
}
