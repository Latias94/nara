//! Actual continuous desktop play, source rebuild, exact adoption and matched native input.

#![cfg(all(feature = "desktop", windows))]
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "support/development_host.rs"]
#[allow(dead_code)]
mod acceptance;
#[path = "support/continuous_desktop.rs"]
mod continuous;
#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod support;

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::Instant,
};

use nara::project_tooling::cargo_build::build_executable;
use two_level_game::{
    development::{
        DevelopmentHost, DevelopmentPlacement, InputActivation, RetirementStatus,
        protocol::{self, ExecutionState, SemanticCommand},
    },
    levels::{LevelId, LevelKey},
};

use acceptance::{
    assert_clean_retirement, assert_preserved, assert_probe_was_discarded, close_to_completion,
    finish_retirements, movement, walk_to_score,
};
use continuous::{
    NativeInput, activate_and_resume, assert_presentation, continuous_native_motion,
    neutral_probes, pause_and_capture, poll_running, status,
};
use support::Fixture;

#[test]
#[ignore = "actual nested Cargo, hardware desktop and matched native D Down/Up; run serially on an idle desktop with physical A/D/W/S initially Up"]
fn production_continuous_desktop_adopts_real_body_and_structure_edits() {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let started = Instant::now();
    let mut input = NativeInput::new();
    let fixture = Fixture::new();
    let request = fixture.desktop_request();
    let parent = fixture.directory("continuous-desktop-host");
    let mut host = DevelopmentHost::with_placement(
        &fixture.project,
        &parent,
        DevelopmentPlacement::SourceDesktop,
    )
    .unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let initial_build = Instant::now();
        let executable = build_executable(&request, |_| Ok(())).unwrap();
        let initial_build_elapsed = initial_build.elapsed();
        host.start(executable).unwrap();
        let a = host.active().unwrap();
        let initial = host.capture().unwrap();
        let fresh = status(&mut host, a, ExecutionState::Paused);
        assert_eq!(fresh.input_activation, InputActivation::Isolated);
        assert!(host.resume_active().is_err());
        assert_preserved(&mut host, &a, &initial);

        walk_to_score(&mut host, 10);
        host.step(vec![movement(false)]).unwrap();
        host.step(vec![SemanticCommand::Marker]).unwrap();
        host.step(vec![SemanticCommand::Travel]).unwrap();
        walk_to_score(&mut host, 20);
        host.step(vec![SemanticCommand::Marker]).unwrap();
        host.step(vec![movement(false)]).unwrap();
        let playable_a = host.capture().unwrap();
        assert_eq!(
            playable_a.progress.location.level,
            LevelKey::main(LevelId::Cavern),
        );
        assert_eq!(playable_a.progress.score, 20);
        assert_eq!(playable_a.progress.markers.len(), 2);
        assert_eq!(playable_a.held, [false; 4]);
        assert_ne!(playable_a.player.translation, [-120.0, 0.0]);
        let presentation = host.confirm_active_presentation().unwrap();
        assert_presentation(&presentation, &playable_a, None);
        let backend = presentation.adapter_backend;

        activate_and_resume(&mut host, &mut input, a);
        assert!(host.capture().is_err());
        assert!(host.step(Vec::new()).is_err());
        assert!(
            host.prepare_rebuild(&request, &playable_a, &neutral_probes(&playable_a, 80.0))
                .is_err(),
        );
        status(&mut host, a, ExecutionState::Running);
        let (basis_a, running_a) =
            continuous_native_motion(&mut host, &mut input, a, &playable_a, 160.0);
        let presentation = host.confirm_active_presentation().unwrap();
        assert_presentation(&presentation, &basis_a, Some(&backend));
        assert_eq!(basis_a.progress.markers, playable_a.progress.markers);
        assert_eq!(basis_a.counters, playable_a.counters);

        let body_edit = Instant::now();
        fixture.edit_body();
        let stale_started = Instant::now();
        let stale = host
            .prepare_rebuild(&request, &basis_a, &neutral_probes(&basis_a, 80.0))
            .unwrap();
        assert_probe_was_discarded(&host, stale.generation());
        assert_presentation(stale.presentation().unwrap(), &basis_a, Some(&backend));
        assert_preserved(&mut host, &a, &basis_a);
        let resumed_a = activate_and_resume(&mut host, &mut input, a);
        assert!(host.adopt(&stale).is_err());
        let running = poll_running(&mut host, a, |observation| {
            observation.logical_tick >= resumed_a.observation.logical_tick + 2
        });
        assert_eq!(running.observation.player, basis_a.player.translation);
        let advanced_a = pause_and_capture(&mut host, a);
        assert!(advanced_a.clock.ticks > basis_a.clock.ticks);
        assert!(host.adopt(&stale).is_err());
        assert_preserved(&mut host, &a, &advanced_a);
        let cancelled = host.cancel(&stale).unwrap();
        finish_retirements(&mut host, cancelled);
        assert_clean_retirement(&host, stale.generation());
        let stale_elapsed = stale_started.elapsed();

        let prepared_b = host
            .prepare_rebuild(&request, &advanced_a, &neutral_probes(&advanced_a, 80.0))
            .unwrap();
        assert_probe_was_discarded(&host, prepared_b.generation());
        assert_presentation(
            prepared_b.presentation().unwrap(),
            &advanced_a,
            Some(&backend),
        );
        assert!(host.adopt(&stale).is_err());
        assert!(host.cancel(&stale).is_err());
        assert_preserved(&mut host, &a, &advanced_a);
        let adopted_b = host.adopt(&prepared_b).unwrap();
        assert_eq!(adopted_b.predecessor, a);
        assert_ne!(adopted_b.active.pid, a.pid);
        assert_ne!(adopted_b.active.executable, a.executable);
        assert_eq!(adopted_b.retirement, RetirementStatus::Pending);
        assert_preserved(&mut host, &adopted_b.active, &advanced_a);
        let presentation = host.confirm_active_presentation().unwrap();
        assert_presentation(&presentation, &advanced_a, Some(&backend));
        assert!(
            host.prepare_rebuild(&request, &advanced_a, &neutral_probes(&advanced_a, 80.0))
                .is_err(),
        );
        assert!(
            host.retirements()
                .iter()
                .all(|report| report.generation != a),
        );
        activate_and_resume(&mut host, &mut input, adopted_b.active);
        let (basis_b, running_b) =
            continuous_native_motion(&mut host, &mut input, adopted_b.active, &advanced_a, 80.0);
        let body_to_playable = body_edit.elapsed();
        assert!(
            host.retirements()
                .iter()
                .all(|report| report.generation != a),
            "new continuous play must precede the explicit predecessor cleanup drive",
        );
        finish_retirements(&mut host, adopted_b.retirement);
        assert_clean_retirement(&host, a);
        assert!(host.adopt(&prepared_b).is_err());
        assert_preserved(&mut host, &adopted_b.active, &basis_b);

        let structure_edit = Instant::now();
        fixture.edit_structure();
        let prepared_c = host
            .prepare_rebuild(&request, &basis_b, &neutral_probes(&basis_b, 80.0))
            .unwrap();
        assert_probe_was_discarded(&host, prepared_c.generation());
        assert_presentation(prepared_c.presentation().unwrap(), &basis_b, Some(&backend));
        let adopted_c = host.adopt(&prepared_c).unwrap();
        assert_eq!(adopted_c.predecessor, adopted_b.active);
        assert_ne!(adopted_c.active.pid, adopted_b.active.pid);
        assert_ne!(adopted_c.active.executable, adopted_b.active.executable);
        assert_eq!(adopted_c.retirement, RetirementStatus::Pending);
        assert_preserved(&mut host, &adopted_c.active, &basis_b);
        let presentation = host.confirm_active_presentation().unwrap();
        assert_presentation(&presentation, &basis_b, Some(&backend));
        activate_and_resume(&mut host, &mut input, adopted_c.active);
        let (basis_c, running_c) =
            continuous_native_motion(&mut host, &mut input, adopted_c.active, &basis_b, 80.0);
        let structure_to_playable = structure_edit.elapsed();
        assert!(
            host.retirements()
                .iter()
                .all(|report| report.generation != adopted_b.active),
        );
        assert_eq!(basis_c.progress.location, basis_a.progress.location);
        assert_eq!(basis_c.progress.markers, basis_a.progress.markers);
        assert_eq!(basis_c.counters, basis_a.counters);
        assert_eq!(basis_c.compatibility, basis_a.compatibility);
        let presentation = host.confirm_active_presentation().unwrap();
        assert_presentation(&presentation, &basis_c, Some(&backend));
        finish_retirements(&mut host, adopted_c.retirement);
        assert_clean_retirement(&host, adopted_b.active);
        assert!(host.adopt(&prepared_c).is_err());
        assert_preserved(&mut host, &adopted_c.active, &basis_c);
        let resumed_c = activate_and_resume(&mut host, &mut input, adopted_c.active);
        let closing = poll_running(&mut host, adopted_c.active, |observation| {
            observation.logical_tick >= resumed_c.observation.logical_tick + 2
        });
        assert_eq!(closing.observation.player, basis_c.player.translation);
        assert_eq!(closing.observation.held, [false; 4]);
        input.assert_foreground();
        eprintln!(
            "continuous_desktop: backend={backend}, initial_build={initial_build_elapsed:?}, \
             live_a={running_a:?}, stale_preparation_resume_refusal_and_cancel={stale_elapsed:?}, \
             body_edit_through_stale_control_to_native_play={body_to_playable:?}, \
             live_b={running_b:?}, structure_edit_to_native_play={structure_to_playable:?}, \
             live_c={running_c:?}, final_execution={:?}, scenario_before_close={:?}",
            closing.state,
            started.elapsed(),
        );
    }));
    let released = input.release();
    let cleanup = catch_unwind(AssertUnwindSafe(|| {
        assert!(
            close_to_completion(&mut host),
            "physical game cleanup remains Pending"
        );
        for report in host.retirements() {
            assert_clean_retirement(&host, report.generation);
        }
        assert!(
            released,
            "the matched native D hold must be released on every outcome"
        );
        assert!(
            input.restore_foreground(),
            "restore the original observed foreground"
        );
    }));
    if let Err(cleanup) = cleanup {
        let root = fixture.root.clone();
        std::mem::forget((host, fixture));
        eprintln!(
            "continuous desktop retains original cleanup owners and source/work copies at {}",
            root.display(),
        );
        resume_unwind(cleanup);
    }
    drop(host);
    if let Err(panic) = result {
        resume_unwind(panic);
    }
}
