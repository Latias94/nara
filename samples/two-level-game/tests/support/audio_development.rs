use std::time::{Duration, Instant};

use nara::identity::SceneEntityId;
use two_level_game::{
    development::{
        ActiveGeneration, DevelopmentHost, InputActivation, ProbeStep, RetirementRole,
        protocol::{AudioStatus, ExecutionState, SemanticCommand},
    },
    levels::{LevelId, LevelKey},
    resume::ResumeState,
    save::CollectedPickup,
};

use crate::{
    acceptance::{
        assert_clean_retirement, assert_observation, assert_preserved, expected_held_tick,
        movement, probe_steps,
    },
    continuous::status,
};

const POLL: Duration = Duration::from_millis(10);
const AUDIO_DEADLINE: Duration = Duration::from_secs(5);
const MAX_AUDIO_POLLS: usize = 128;

pub fn audio(host: &mut DevelopmentHost, generation: ActiveGeneration) -> AudioStatus {
    assert_eq!(host.active(), Some(generation));
    let audio = host.audio_status().unwrap();
    assert_eq!(host.active(), Some(generation));
    assert!(
        audio.runtime_generation > 0 && audio.reset_on_rebuild,
        "{audio:?}",
    );
    assert!(!audio.failed, "{audio:?}");
    assert_eq!(audio.faults, 0, "{audio:?}");
    assert!(audio.admission_open, "{audio:?}");
    assert!(!audio.physical_complete, "{audio:?}");
    assert!(!audio.stream_drop_returned, "{audio:?}");
    assert!(!audio.data_callback_released, "{audio:?}");
    assert!(!audio.error_callback_released, "{audio:?}");
    audio
}

pub fn assert_suppressed(audio: &AudioStatus) {
    assert!(!audio.output_enabled, "{audio:?}");
    assert_eq!(audio.accepted, 0, "{audio:?}");
    assert_eq!(audio.started, 0, "{audio:?}");
    assert_eq!(audio.completed, 0, "{audio:?}");
    assert_eq!(audio.cancelled, 0, "{audio:?}");
}

pub fn assert_pending_audio(
    host: &mut DevelopmentHost,
    predecessor: ActiveGeneration,
    active: ActiveGeneration,
    canonical: &ResumeState,
) -> AudioStatus {
    assert!(
        host.retirements()
            .iter()
            .all(|report| report.generation != predecessor),
    );
    let before = audio(host, active);
    assert_suppressed(&before);
    assert!(host.enable_active_audio().is_err());
    assert!(host.activate_active().is_err());
    assert!(host.resume_active().is_err());
    let after = audio(host, active);
    assert_suppressed(&after);
    assert_eq!(after.suppressed_effects, before.suppressed_effects);
    assert_eq!(after.runtime_generation, before.runtime_generation);
    let paused = status(host, active, ExecutionState::Paused);
    assert_eq!(paused.input_activation, InputActivation::Isolated);
    assert_preserved(host, &active, canonical);
    assert!(
        host.retirements()
            .iter()
            .all(|report| report.generation != predecessor),
    );
    after
}

pub fn assert_audio_retirement(
    host: &DevelopmentHost,
    generation: ActiveGeneration,
) -> AudioStatus {
    assert_clean_retirement(host, generation);
    let report = host
        .retirements()
        .iter()
        .find(|report| report.generation == generation)
        .unwrap();
    let audio = report
        .audio
        .expect("the original audible child must report final audio retirement");
    assert!(
        audio.runtime_generation > 0 && audio.reset_on_rebuild,
        "{report:?}",
    );
    assert!(!audio.failed, "{report:?}");
    assert_eq!(audio.faults, 0, "{report:?}");
    assert!(!audio.output_enabled, "{report:?}");
    assert!(!audio.admission_open, "{report:?}");
    assert!(audio.physical_complete, "{report:?}");
    assert!(audio.stream_drop_returned, "{report:?}");
    assert!(audio.data_callback_released, "{report:?}");
    assert!(audio.error_callback_released, "{report:?}");
    assert!(audio.callbacks > 0 && audio.output_frames > 0, "{report:?}");
    assert_eq!(
        audio.accepted,
        audio.completed + audio.cancelled,
        "{report:?}",
    );
    if matches!(
        report.role,
        RetirementRole::Probe | RetirementRole::Candidate
    ) {
        assert_suppressed(&audio);
    }
    audio
}

/// Observe maintenance only after all earlier accepted effects have physically finished.
pub fn assert_no_replay(
    host: &mut DevelopmentHost,
    generation: ActiveGeneration,
    canonical: &ResumeState,
) -> AudioStatus {
    let before = audio(host, generation);
    assert_eq!(
        before.accepted,
        before.completed + before.cancelled,
        "settle earlier effects before requiring all counters to remain unchanged",
    );
    for _ in 0..4 {
        let observation = host.frame().unwrap();
        assert_observation(&observation, canonical);
        let current = audio(host, generation);
        assert_eq!(current.runtime_generation, before.runtime_generation);
        assert_eq!(current.output_enabled, before.output_enabled);
        assert_eq!(
            current.accepted, before.accepted,
            "maintenance must not replay past effects",
        );
        assert_eq!(current.started, before.started);
        assert_eq!(current.completed, before.completed);
        assert_eq!(current.cancelled, before.cancelled);
        assert_eq!(current.suppressed_effects, before.suppressed_effects);
        std::thread::sleep(POLL);
    }
    assert_preserved(host, &generation, canonical);
    audio(host, generation)
}

pub fn wait_for_fresh_effect(
    host: &mut DevelopmentHost,
    generation: ActiveGeneration,
    before: AudioStatus,
) -> AudioStatus {
    assert!(before.output_enabled, "{before:?}");
    let canonical = host.capture().unwrap();
    let deadline = Instant::now() + AUDIO_DEADLINE;
    for _ in 0..MAX_AUDIO_POLLS {
        let current = audio(host, generation);
        assert_eq!(current.runtime_generation, before.runtime_generation);
        assert!(current.output_enabled, "{current:?}");
        assert!(current.accepted <= before.accepted + 1, "{current:?}");
        assert_eq!(current.suppressed_effects, before.suppressed_effects);
        assert_eq!(current.cancelled, before.cancelled);
        if current.accepted == before.accepted + 1
            && current.started == before.started + 1
            && current.completed == before.completed + 1
            && current.callbacks > before.callbacks
            && current.output_frames > before.output_frames
        {
            assert_preserved(host, &generation, &canonical);
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "fresh effect did not finish: {current:?}",
        );
        let observation = host.frame().unwrap();
        assert_observation(&observation, &canonical);
        std::thread::sleep(POLL);
    }
    panic!("fresh effect exceeded its finite callback observation budget");
}

pub fn idle_tick(base: &ResumeState) -> ResumeState {
    assert_eq!(base.held, [false; 4]);
    let mut expected = base.clone();
    expected.clock.ticks += 1;
    expected.clock.elapsed_nanos += expected.clock.timestep_nanos;
    expected.validate().unwrap();
    expected
}

pub fn right_tick(base: &ResumeState, speed: f32) -> ResumeState {
    assert_eq!(base.held, [false; 4]);
    let mut held = base.clone();
    held.held = [false, true, false, false];
    expected_held_tick(&held, speed)
}

pub fn release_tick(base: &ResumeState) -> ResumeState {
    assert_eq!(base.held, [false, true, false, false]);
    let mut released = base.clone();
    released.held = [false; 4];
    idle_tick(&released)
}

/// Stage the unchanged A movement law beside Meadow's still-uncollected authored far gem.
pub fn stage_pickup(host: &mut DevelopmentHost, generation: ActiveGeneration) -> ResumeState {
    let before = host.capture().unwrap();
    assert_eq!(
        before.progress.location.level,
        LevelKey::main(LevelId::Meadow),
    );
    assert_eq!(before.progress.score, 20);
    assert_eq!(before.held, [false; 4]);
    assert_eq!(before.player.translation, [-120.0, 0.0]);
    let mut observation = host
        .step(vec![SemanticCommand::Move {
            direction: 2,
            pressed: true,
        }])
        .unwrap();
    for _ in 0..24 {
        if observation.player[1] >= 38.0 {
            break;
        }
        observation = host.step(Vec::new()).unwrap();
    }
    assert!(
        (38.0..42.0).contains(&observation.player[1]),
        "{observation:?}",
    );
    host.step(vec![SemanticCommand::Move {
        direction: 2,
        pressed: false,
    }])
    .unwrap();
    observation = host.step(vec![movement(true)]).unwrap();
    for _ in 0..96 {
        if observation.player[0] >= 54.0 {
            break;
        }
        observation = host.step(Vec::new()).unwrap();
    }
    assert!(
        (54.0..60.0).contains(&observation.player[0]),
        "{observation:?}",
    );
    let observation = host.step(vec![movement(false)]).unwrap();
    let staged = host.capture().unwrap();
    assert_observation(&observation, &staged);
    assert!((38.0..42.0).contains(&staged.player.translation[1]));
    assert_eq!(staged.progress, before.progress);
    assert_eq!(staged.counters, before.counters);
    assert_eq!(staged.compatibility, before.compatibility);
    assert_eq!(staged.held, [false; 4]);
    status(host, generation, ExecutionState::Paused);
    staged
}

/// Complete independent states for actual movement, one authored pickup, a marker and release.
pub fn pickup_probes(base: &ResumeState, speed: f32) -> Vec<ProbeStep> {
    assert_eq!(
        base.progress.location.level,
        LevelKey::main(LevelId::Meadow),
    );
    assert_eq!(base.progress.score, 20);
    assert_eq!(base.held, [false; 4]);
    assert!((54.0..60.0).contains(&base.player.translation[0]));
    assert!((38.0..42.0).contains(&base.player.translation[1]));
    let pickup = CollectedPickup {
        level: base.progress.location.level,
        entity: SceneEntityId::new("gem-far").unwrap(),
    };
    assert!(!base.progress.collected.contains(&pickup));
    let mut expected = base.clone();
    expected.held = [false, true, false, false];
    let mut probes = Vec::new();
    for index in 0..16 {
        expected = expected_held_tick(&expected, speed);
        let [x, y] = expected.player.translation;
        if ((x - 80.0).powi(2) + (y - 40.0).powi(2)).sqrt() <= 16.0 {
            expected.progress.collected.push(pickup.clone());
            expected.progress.score += 10;
        }
        expected.validate().unwrap();
        probes.push(ProbeStep {
            commands: if index == 0 {
                vec![movement(true)]
            } else {
                Vec::new()
            },
            expected: expected.clone(),
        });
        if expected.progress.collected.contains(&pickup) {
            probes.extend(probe_steps(&expected, speed));
            assert!(probes.len() <= 64);
            return probes;
        }
    }
    panic!("the finite independent pickup oracle did not reach the authored gem");
}
