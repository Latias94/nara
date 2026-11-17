//! Actual CPAL desktop development, silent qualification and original-owner retirement.

#![cfg(all(
    windows,
    feature = "development",
    feature = "desktop",
    feature = "audio-cpal"
))]
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "support/development_host.rs"]
#[allow(dead_code)]
mod acceptance;
#[path = "support/audio_development.rs"]
mod audible;
#[path = "support/continuous_desktop.rs"]
#[allow(dead_code)]
mod continuous;
#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod support;

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use nara::{
    project_tooling::cargo_build::{CargoBuildFailure, build_executable},
    project_workflow::cargo::CargoBuildError,
};
use two_level_game::{
    development::{
        DevelopmentHost, DevelopmentPlacement, DevelopmentStage, InputActivation, RetirementRole,
        RetirementStatus,
        protocol::{self, ExecutionState, SemanticCommand},
    },
    levels::{LevelId, LevelKey},
};

use acceptance::{
    assert_observation, assert_preserved, assert_probe_was_discarded, close_to_completion,
    finish_retirements, movement, walk_to_score,
};
use audible::{
    assert_audio_retirement, assert_no_replay, assert_pending_audio, assert_suppressed, audio,
    idle_tick, pickup_probes, release_tick, right_tick, stage_pickup, wait_for_fresh_effect,
};
use continuous::{
    NativeInput, activate_and_resume, assert_presentation, continuous_native_motion,
    neutral_probes, pause_and_capture, poll_running, status,
};
use support::{COMPILE_ERROR, Fixture, GAMEPLAY};

#[test]
#[ignore = "actual nested Cargo, CPAL/WASAPI and hardware Windows desktop; run serially with root target, one build job and physical A/D/W/S Up"]
fn production_audible_development_adopts_real_edits_after_original_audio_retirement() {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let parent_backend_request = std::env::var("WGPU_BACKEND").ok();
    let mut input = NativeInput::new();
    let fixture = Fixture::new();
    let source_a = fixture.read(GAMEPLAY);
    let request = fixture.audio_desktop_request();
    assert_eq!(
        request.selection().features(),
        ["audio-cpal", "desktop", "development"],
    );
    let parent = fixture.directory("audible-development-host");
    let mut host = DevelopmentHost::with_placement(
        &fixture.project,
        &parent,
        DevelopmentPlacement::SourceDesktopAudio,
    )
    .unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let executable = build_executable(&request, |_| Ok(())).unwrap();
        assert_eq!(executable.provenance().selection(), request.selection());
        let ready = host.start(executable).unwrap();
        let a = host.active().unwrap();
        assert_eq!(host.placement(), DevelopmentPlacement::SourceDesktopAudio);
        let initial = host.capture().unwrap();
        assert_observation(&ready, &initial);
        let initial_audio = audio(&mut host, a);
        assert_suppressed(&initial_audio);
        assert_eq!(initial_audio.runtime_generation, ready.runtime_generation);
        let paused = status(&mut host, a, ExecutionState::Paused);
        assert_eq!(paused.input_activation, InputActivation::Isolated);
        assert!(host.resume_active().is_err());
        assert_preserved(&mut host, &a, &initial);

        walk_to_score(&mut host, 10);
        host.step(vec![movement(false)]).unwrap();
        host.step(vec![SemanticCommand::Marker]).unwrap();
        host.step(vec![SemanticCommand::Travel]).unwrap();
        walk_to_score(&mut host, 20);
        host.step(vec![SemanticCommand::Marker]).unwrap();
        host.step(vec![movement(false)]).unwrap();
        let meaningful = host.capture().unwrap();
        assert_eq!(
            meaningful.progress.location.level,
            LevelKey::main(LevelId::Cavern),
        );
        assert_eq!(meaningful.progress.score, 20);
        assert_eq!(meaningful.progress.collected.len(), 2);
        assert_eq!(meaningful.progress.markers.len(), 2);
        assert_eq!(meaningful.progress.next_marker_id.get(), 3);
        assert_eq!(meaningful.counters.travel_count, 1);
        assert_eq!(meaningful.held, [false; 4]);
        assert_ne!(meaningful.player.translation, [-120.0, 0.0]);
        let suppressed_a = audio(&mut host, a);
        assert_suppressed(&suppressed_a);
        assert!(suppressed_a.suppressed_effects >= initial_audio.suppressed_effects + 3);
        let presentation = host.confirm_active_presentation().unwrap();
        assert_presentation(&presentation, &meaningful, None);
        let backend = presentation.adapter_backend.clone();
        assert_eq!(
            presentation.runtime_generation,
            suppressed_a.runtime_generation,
        );

        let enabled_a = host.enable_active_audio().unwrap();
        assert!(enabled_a.output_enabled);
        assert_eq!(enabled_a.accepted, suppressed_a.accepted);
        assert_eq!(
            enabled_a.suppressed_effects,
            suppressed_a.suppressed_effects
        );
        let before_travel = assert_no_replay(&mut host, a, &meaningful);
        host.step(vec![SemanticCommand::Travel]).unwrap();
        let sounded_a = wait_for_fresh_effect(&mut host, a, before_travel);
        let returned_a = host.capture().unwrap();
        assert_eq!(
            returned_a.progress.location.level,
            LevelKey::main(LevelId::Meadow),
        );
        assert_eq!(returned_a.progress.score, 20);
        assert_eq!(returned_a.progress.markers, meaningful.progress.markers);
        assert_eq!(returned_a.counters.travel_count, 2);
        let resumed_a = activate_and_resume(&mut host, &mut input, a);
        poll_running(&mut host, a, |observation| {
            observation.logical_tick >= resumed_a.observation.logical_tick + 2
        });
        let paused_a = pause_and_capture(&mut host, a);
        assert_eq!(paused_a.progress, returned_a.progress);
        assert_eq!(paused_a.player, returned_a.player);
        assert_eq!(paused_a.counters, returned_a.counters);
        assert_eq!(paused_a.compatibility, returned_a.compatibility);
        assert!(paused_a.clock.ticks > returned_a.clock.ticks);
        assert_eq!(
            assert_no_replay(&mut host, a, &paused_a).accepted,
            sounded_a.accepted,
        );

        let staged = stage_pickup(&mut host, a);
        let presentation = host.confirm_active_presentation().unwrap();
        assert_presentation(&presentation, &staged, Some(&backend));
        fixture.write(
            GAMEPLAY,
            &format!("{source_a}\ncompile_error!({COMPILE_ERROR:?});\n"),
        );
        let error = host
            .prepare_rebuild(&request, &staged, &pickup_probes(&staged, 80.0))
            .unwrap_err();
        assert_eq!(error.stage, DevelopmentStage::Build);
        assert!(
            matches!(host.last_build_failure(),
                Some(CargoBuildFailure::Compilation { status, diagnostics, protocol })
                    if !status.success()
                        && matches!(protocol, Some(CargoBuildError::BuildFailed))
                        && diagnostics.entries().iter().any(|entry| entry.message() == COMPILE_ERROR)),
            "retain the actual compiler failure: {:?}",
            host.last_build_failure(),
        );
        assert_preserved(&mut host, &a, &staged);
        let continued_a = host.step(vec![movement(true)]).unwrap();
        let moved_a = right_tick(&staged, 160.0);
        assert_observation(&continued_a, &moved_a);
        assert_preserved(&mut host, &a, &moved_a);
        let released_a = host.step(vec![movement(false)]).unwrap();
        let mut basis_a = release_tick(&moved_a);
        assert_observation(&released_a, &basis_a);
        assert_preserved(&mut host, &a, &basis_a);
        assert_eq!(audio(&mut host, a).accepted, sounded_a.accepted);
        fixture.write(GAMEPLAY, &source_a);
        fixture.edit_body();

        let mut refused = pickup_probes(&basis_a, 80.0);
        let final_expected = &mut refused.last_mut().unwrap().expected;
        final_expected.counters.saves += 1;
        final_expected.validate().unwrap();
        let error = host
            .prepare_rebuild(&request, &basis_a, &refused)
            .unwrap_err();
        assert_eq!(error.stage, DevelopmentStage::Probe);
        let retirement = host.retry_retirement();
        finish_retirements(&mut host, retirement);
        let rejected = host.retirements().last().unwrap();
        assert_eq!(rejected.role, RetirementRole::Probe);
        assert_ne!(rejected.generation.pid, a.pid);
        assert_ne!(rejected.generation.executable, a.executable);
        let rejected_audio = assert_audio_retirement(&host, rejected.generation);
        assert_suppressed(&rejected_audio);
        assert!(rejected_audio.suppressed_effects > 0);
        assert_preserved(&mut host, &a, &basis_a);
        assert_eq!(audio(&mut host, a).accepted, sounded_a.accepted);

        let cancelled = host
            .prepare_rebuild(&request, &basis_a, &pickup_probes(&basis_a, 80.0))
            .unwrap();
        assert_probe_was_discarded(&host, cancelled.generation());
        assert_presentation(cancelled.presentation().unwrap(), &basis_a, Some(&backend));
        assert_preserved(&mut host, &a, &basis_a);
        let retirement = host.cancel(&cancelled).unwrap();
        finish_retirements(&mut host, retirement);
        assert_suppressed(&assert_audio_retirement(&host, cancelled.generation()));
        assert!(host.adopt(&cancelled).is_err());
        assert!(host.cancel(&cancelled).is_err());
        assert_preserved(&mut host, &a, &basis_a);

        let stale = host
            .prepare_rebuild(&request, &basis_a, &pickup_probes(&basis_a, 80.0))
            .unwrap();
        assert_probe_was_discarded(&host, stale.generation());
        assert_presentation(stale.presentation().unwrap(), &basis_a, Some(&backend));
        let continued_a = host.step(Vec::new()).unwrap();
        basis_a = idle_tick(&basis_a);
        assert_observation(&continued_a, &basis_a);
        assert!(host.adopt(&stale).is_err());
        assert_preserved(&mut host, &a, &basis_a);
        let retirement = host.cancel(&stale).unwrap();
        finish_retirements(&mut host, retirement);
        assert_suppressed(&assert_audio_retirement(&host, stale.generation()));
        assert!(host.adopt(&stale).is_err());
        assert!(host.cancel(&stale).is_err());
        assert_preserved(&mut host, &a, &basis_a);

        let b_probes = pickup_probes(&basis_a, 80.0);
        let prepared_b = host.prepare_rebuild(&request, &basis_a, &b_probes).unwrap();
        assert_probe_was_discarded(&host, prepared_b.generation());
        assert_presentation(prepared_b.presentation().unwrap(), &basis_a, Some(&backend));
        assert!(host.adopt(&cancelled).is_err());
        assert!(host.cancel(&stale).is_err());
        assert_preserved(&mut host, &a, &basis_a);
        let adopted_b = host.adopt(&prepared_b).unwrap();
        let b = adopted_b.active;
        assert_eq!(b, prepared_b.generation());
        assert_eq!(adopted_b.predecessor, a);
        assert_ne!(b.pid, a.pid);
        assert_ne!(b.executable, a.executable);
        assert_ne!(b.provenance, a.provenance);
        assert!(b.sequence > a.sequence);
        assert_eq!(adopted_b.retirement, RetirementStatus::Pending);
        assert_preserved(&mut host, &b, &basis_a);
        assert_presentation(
            &host.confirm_active_presentation().unwrap(),
            &basis_a,
            Some(&backend),
        );
        let muted_b = assert_pending_audio(&mut host, a, b, &basis_a);
        for probe in &b_probes {
            let observation = host.step(probe.commands.clone()).unwrap();
            assert_observation(&observation, &probe.expected);
            assert_preserved(&mut host, &b, &probe.expected);
        }
        let basis_b = b_probes.last().unwrap().expected.clone();
        assert_eq!(basis_b.progress.score, 30);
        assert_eq!(basis_b.progress.markers.len(), 3);
        assert_eq!(basis_b.progress.next_marker_id.get(), 4);
        let suppressed_b = assert_pending_audio(&mut host, a, b, &basis_b);
        assert!(suppressed_b.suppressed_effects > muted_b.suppressed_effects);
        assert_presentation(
            &host.confirm_active_presentation().unwrap(),
            &basis_b,
            Some(&backend),
        );
        finish_retirements(&mut host, adopted_b.retirement);
        let retired_a = assert_audio_retirement(&host, a);
        assert_eq!(retired_a.accepted, sounded_a.accepted);
        assert_eq!(
            retired_a.suppressed_effects,
            suppressed_a.suppressed_effects
        );
        assert!(host.adopt(&prepared_b).is_err());
        assert!(host.cancel(&prepared_b).is_err());
        let enabled_b = host.enable_active_audio().unwrap();
        assert!(enabled_b.output_enabled);
        assert_eq!(enabled_b.accepted, suppressed_b.accepted);
        assert_eq!(
            enabled_b.suppressed_effects,
            suppressed_b.suppressed_effects
        );
        let before_b_travel = assert_no_replay(&mut host, b, &basis_b);
        host.step(vec![SemanticCommand::Travel]).unwrap();
        let sounded_b = wait_for_fresh_effect(&mut host, b, before_b_travel);
        let restored_b = host.capture().unwrap();
        assert_eq!(
            restored_b.progress.location.level,
            LevelKey::main(LevelId::Cavern),
        );
        assert_eq!(restored_b.progress.score, 30);
        assert_eq!(restored_b.progress.markers, basis_b.progress.markers);
        assert_eq!(
            restored_b.counters.travel_count,
            basis_b.counters.travel_count + 1,
        );

        let changed = fixture.edit_structure();
        assert!(changed.contains(GAMEPLAY));
        let c_probes = neutral_probes(&restored_b, 80.0);
        let prepared_c = host
            .prepare_rebuild(&request, &restored_b, &c_probes)
            .unwrap();
        assert_probe_was_discarded(&host, prepared_c.generation());
        assert_presentation(
            prepared_c.presentation().unwrap(),
            &restored_b,
            Some(&backend),
        );
        assert_preserved(&mut host, &b, &restored_b);
        let adopted_c = host.adopt(&prepared_c).unwrap();
        let c = adopted_c.active;
        assert_eq!(c, prepared_c.generation());
        assert_eq!(adopted_c.predecessor, b);
        assert_ne!(c.pid, b.pid);
        assert_ne!(c.executable, b.executable);
        assert_ne!(c.provenance, b.provenance);
        assert!(c.sequence > b.sequence);
        assert_eq!(adopted_c.retirement, RetirementStatus::Pending);
        assert_preserved(&mut host, &c, &restored_b);
        assert_presentation(
            &host.confirm_active_presentation().unwrap(),
            &restored_b,
            Some(&backend),
        );
        assert_pending_audio(&mut host, b, c, &restored_b);
        for probe in &c_probes {
            let observation = host.step(probe.commands.clone()).unwrap();
            assert_observation(&observation, &probe.expected);
            assert_preserved(&mut host, &c, &probe.expected);
        }
        let basis_c = c_probes.last().unwrap().expected.clone();
        let muted_c = assert_pending_audio(&mut host, b, c, &basis_c);
        assert_presentation(
            &host.confirm_active_presentation().unwrap(),
            &basis_c,
            Some(&backend),
        );
        finish_retirements(&mut host, adopted_c.retirement);
        let retired_b = assert_audio_retirement(&host, b);
        assert_eq!(retired_b.accepted, sounded_b.accepted);
        assert_eq!(
            retired_b.suppressed_effects,
            suppressed_b.suppressed_effects
        );
        assert!(host.adopt(&prepared_b).is_err());
        assert!(host.cancel(&prepared_c).is_err());
        assert!(host.adopt(&prepared_c).is_err());
        assert_preserved(&mut host, &c, &basis_c);

        activate_and_resume(&mut host, &mut input, c);
        let activated_c = audio(&mut host, c);
        assert!(activated_c.output_enabled);
        assert_eq!(activated_c.accepted, muted_c.accepted);
        assert_eq!(activated_c.suppressed_effects, muted_c.suppressed_effects);
        let (continued_c, _) = continuous_native_motion(&mut host, &mut input, c, &basis_c, 80.0);
        assert_eq!(continued_c.progress, basis_c.progress);
        assert_eq!(continued_c.counters, basis_c.counters);
        assert_eq!(continued_c.compatibility, basis_c.compatibility);
        assert_eq!(continued_c.held, [false; 4]);
        assert_presentation(
            &host.confirm_active_presentation().unwrap(),
            &continued_c,
            Some(&backend),
        );
        let before_c_travel = assert_no_replay(&mut host, c, &continued_c);
        assert_eq!(before_c_travel.accepted, muted_c.accepted);
        host.step(vec![SemanticCommand::Travel]).unwrap();
        let sounded_c = wait_for_fresh_effect(&mut host, c, before_c_travel);
        let final_state = host.capture().unwrap();
        assert_eq!(
            final_state.progress.location.level,
            LevelKey::main(LevelId::Meadow),
        );
        assert_eq!(final_state.progress.score, 30);
        assert_eq!(final_state.progress.markers.len(), 4);
        assert_eq!(final_state.progress.next_marker_id.get(), 5);
        assert_eq!(final_state.counters.travel_count, 4);
        assert_presentation(
            &host.confirm_active_presentation().unwrap(),
            &final_state,
            Some(&backend),
        );
        assert_eq!(
            assert_no_replay(&mut host, c, &final_state).accepted,
            sounded_c.accepted,
        );
        assert_preserved(&mut host, &c, &final_state);
        input.assert_foreground();
        eprintln!(
            "audible_development: backend={backend}, \
             parent_backend_request={parent_backend_request:?}, rebuild_attempts=6, \
             initial_suppressed={}, rejected_probe_suppressed={}, \
             adopted_b_suppressed={}, accepted_a={}, accepted_b={}, accepted_c={}; \
             original device callback and retirement evidence only",
            suppressed_a.suppressed_effects,
            rejected_audio.suppressed_effects,
            suppressed_b.suppressed_effects,
            retired_a.accepted,
            retired_b.accepted,
            sounded_c.accepted,
        );
        (c, sounded_c)
    }));
    let cleanup = catch_unwind(AssertUnwindSafe(|| {
        let released = input.release();
        let closed = close_to_completion(&mut host);
        let restored_foreground = input.restore_foreground();
        assert!(released, "release the original matched native D hold");
        assert!(closed, "original game/audio/native cleanup remains Pending");
        assert!(
            restored_foreground,
            "restore the original observed foreground"
        );
        assert!(host.active().is_none());
        for report in host.retirements() {
            assert_audio_retirement(&host, report.generation);
        }
        if let Ok((generation, before_close)) = &result {
            let retired = assert_audio_retirement(&host, *generation);
            assert_eq!(retired.accepted, before_close.accepted);
            assert_eq!(retired.started, before_close.started);
            assert_eq!(retired.completed, before_close.completed);
            assert_eq!(retired.cancelled, before_close.cancelled);
            assert_eq!(retired.suppressed_effects, before_close.suppressed_effects);
            let report = host
                .retirements()
                .iter()
                .find(|report| report.generation == *generation)
                .unwrap();
            assert_eq!(report.role, RetirementRole::Active);
        }
    }));
    if let Err(cleanup) = cleanup {
        let root = fixture.root.clone();
        std::mem::forget((host, fixture, input));
        eprintln!(
            "audible development retains original Host, source/work copies and native \
             input/foreground owner at {}",
            root.display(),
        );
        if let Err(original) = result {
            eprintln!("original trajectory failed before its retained cleanup: {cleanup:?}");
            resume_unwind(original);
        }
        resume_unwind(cleanup);
    }
    drop(host);
    if let Err(panic) = result {
        resume_unwind(panic);
    }
}

#[test]
#[ignore = "actual nested Cargo, CPAL/WASAPI and hardware Windows desktop; run serially with root target and one build job"]
fn a_failed_game_close_preserves_its_error_without_blocking_physically_retired_audio() {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let fixture = Fixture::new();
    let original = fixture.read(GAMEPLAY);
    let finish = "    fn finish(&self, app: &mut App) -> Result<(), PluginError> {";
    assert_eq!(original.matches(finish).count(), 1);
    let failed_shutdown = "    fn shutdown(&self, _: &mut nara::app::PluginShutdownContext<'_>) -> Result<(), PluginError> {\n        Err(PluginError::LifecycleShutdown)\n    }\n\n";
    fixture.write(
        GAMEPLAY,
        &original.replacen(finish, &format!("{failed_shutdown}{finish}"), 1),
    );
    let request = fixture.audio_desktop_request();
    let parent = fixture.directory("audible-failed-predecessor");
    let mut host = DevelopmentHost::with_placement(
        &fixture.project,
        &parent,
        DevelopmentPlacement::SourceDesktopAudio,
    )
    .unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        host.start(build_executable(&request, |_| Ok(())).unwrap())
            .unwrap();
        let a = host.active().unwrap();
        let basis = host.capture().unwrap();
        assert_suppressed(&audio(&mut host, a));
        fixture.write(GAMEPLAY, &original);
        let prepared = host
            .prepare_rebuild(&request, &basis, &neutral_probes(&basis, 160.0))
            .unwrap();
        assert_probe_was_discarded(&host, prepared.generation());
        let adopted = host.adopt(&prepared).unwrap();
        let b = adopted.active;
        assert_eq!(adopted.predecessor, a);
        assert_eq!(adopted.retirement, RetirementStatus::Pending);
        assert_preserved(&mut host, &b, &basis);
        finish_retirements(&mut host, adopted.retirement);
        let report = host
            .retirements()
            .iter()
            .find(|report| report.generation == a)
            .unwrap();
        assert_eq!(report.role, RetirementRole::Predecessor);
        assert!(!report.game_closed && !report.terminated, "{report:?}");
        assert!(!report.native.exit().success(), "{report:?}");
        assert!(
            report.native.containment_empty()
                && report.native.readers_stopped()
                && report.native.output_drained(),
            "{report:?}"
        );
        let retired = report.audio.unwrap();
        assert!(!retired.failed && retired.faults == 0, "{retired:?}");
        assert!(
            !retired.admission_open && !retired.output_enabled,
            "{retired:?}"
        );
        assert!(
            retired.physical_complete && retired.stream_drop_returned,
            "{retired:?}"
        );
        assert!(
            retired.data_callback_released && retired.error_callback_released,
            "{retired:?}"
        );
        assert_suppressed(&retired);
        assert_suppressed(&audio(&mut host, b));
        assert_preserved(&mut host, &b, &basis);
        let next = idle_tick(&basis);
        assert_observation(&host.step(Vec::new()).unwrap(), &next);
        assert_preserved(&mut host, &b, &next);
        assert_suppressed(&audio(&mut host, b));
        let enabled = host.enable_active_audio().unwrap();
        assert!(enabled.output_enabled);
        assert_eq!(enabled.accepted, 0);
        assert_no_replay(&mut host, b, &next);
        host.step(vec![SemanticCommand::Travel]).unwrap();
        let sounded = wait_for_fresh_effect(&mut host, b, enabled);
        assert_eq!(sounded.accepted, 1);
        let failed = host
            .retirements()
            .iter()
            .find(|report| report.generation == a)
            .unwrap();
        assert!(!failed.game_closed && !failed.native.exit().success());
        assert_eq!(host.active(), Some(b));
        b
    }));
    if !close_to_completion(&mut host) {
        let root = fixture.root.clone();
        std::mem::forget((host, fixture));
        panic!(
            "failed-predecessor control retains original cleanup owners at {}",
            root.display()
        );
    }
    if let Ok(active) = &result {
        assert_audio_retirement(&host, *active);
    }
    drop(host);
    if let Err(error) = result {
        resume_unwind(error);
    }
}
