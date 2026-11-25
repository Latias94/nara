use std::{
    collections::BTreeMap,
    fs,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nara::{
    app::{
        PluginCategory, PluginDeclaration, PluginId, RuntimeCandidateRetirementState,
        RuntimeInstance, StartupStage,
    },
    ecs::{World, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    prelude::{App, Plugin, PluginError},
};

use super::{AudioWitness, isolated_audio, wait};
use crate::{
    GameResumeError, RunOptions,
    audio::{self, AudioCause, AudioOptions, AudioPolicy, AudioRecord, AudioSnapshot},
    levels::{LevelId, LevelKey},
    resume::encode_resume,
};
use support::{Game, MAX_WALK_STEPS, ProjectCopy, TRAVEL, action, movement};

mod oracle {
    mod two_level_game {
        pub use crate::{audio, levels};
    }

    include!("../bin/audio_desktop/oracle.rs");
}

#[allow(
    dead_code,
    reason = "reuse the original public game owner and command/cleanup helpers"
)]
mod support {
    use crate as two_level_game;

    include!("../../tests/support/resume_gameplay.rs");
}

#[derive(Default)]
struct Capture(Vec<f32>);

impl Capture {
    fn drain(&mut self, witness: &AudioWitness) -> AudioSnapshot {
        for chunk in witness.drain_output() {
            assert_eq!(chunk.channels, 2);
            assert_eq!(chunk.first_channel, 0);
            assert_eq!(chunk.first_frame, (self.0.len() / 2) as u64);
            assert!(!chunk.samples.is_empty());
            assert!(chunk.samples.len().is_multiple_of(2));
            assert!(self.0.len() + chunk.samples.len() <= oracle::MAX_OUTPUT_FRAMES as usize * 2);
            self.0.extend(chunk.samples);
        }
        let snapshot = witness.snapshot();
        assert_eq!(snapshot.faults, 0, "{snapshot:?}");
        assert!(snapshot.error.is_none(), "{snapshot:?}");
        snapshot
    }

    fn assert_silent(&self) {
        assert!(!self.0.is_empty(), "no actual CPAL samples were captured");
        assert!(self.0.iter().all(|sample| *sample == 0.0));
    }
}

fn observation(game: &Game) -> AudioWitness {
    game.session
        .observe(|world| audio::witness(world).unwrap())
        .unwrap()
}

fn remember(game: &Game, instances: &mut BTreeMap<u64, LevelKey>) {
    let snapshot = game.snapshot();
    instances.insert(
        snapshot.current_instance.unwrap().get(),
        snapshot.level.unwrap(),
    );
}

fn collect_near_pickup(game: &mut Game, score: u32, witness: &AudioWitness, capture: &mut Capture) {
    game.command(movement(1, true));
    for _ in 0..MAX_WALK_STEPS {
        capture.drain(witness);
        if game.snapshot().score == score {
            break;
        }
        assert!(game.snapshot().score < score);
        game.step();
    }
    assert_eq!(game.snapshot().score, score);
    game.command(movement(1, false));
    capture.drain(witness);
}

#[test]
#[ignore = "requires an actual Windows stereo 48 kHz f32 output device; run serially"]
fn a_real_development_game_is_silent_until_enabled_and_only_new_cues_match_the_full_pcm_oracle() {
    let project = ProjectCopy::new();
    let pickup = fs::read(project.root().join("assets/audio/pickup.wav")).unwrap();
    let travel = fs::read(project.root().join("assets/audio/travel.wav")).unwrap();
    let sources = oracle::SourceWavs {
        pickup: &pickup,
        travel: &travel,
    };
    let recipe = || {
        audio::development_recipe(
            RunOptions::default(),
            AudioOptions {
                capture_output: true,
            },
        )
    };
    let mut game = Game::from_project_with_recipe(project.root(), recipe());
    let witness = observation(&game);
    let initial = witness.snapshot();
    assert!(initial.reset_on_rebuild && !initial.output_enabled);
    assert!(initial.play_admitted && initial.admission_open);
    let mut capture = Capture::default();
    let mut instances = BTreeMap::new();
    remember(&game, &mut instances);
    collect_near_pickup(&mut game, 10, &witness, &mut capture);
    game.command(action(TRAVEL));
    remember(&game, &mut instances);
    assert_eq!(game.snapshot().level, Some(LevelKey::main(LevelId::Cavern)));
    let saved = game.capture();
    wait(
        || {
            let snapshot = capture.drain(&witness);
            !capture.0.is_empty() && snapshot.desired_instance == snapshot.applied_instance
        },
        "disabled gameplay scope was not applied by the real callback",
    );
    let disabled = capture.drain(&witness);
    assert!(!disabled.output_enabled);
    assert_eq!(disabled.suppressed_effects, 2);
    assert_eq!(
        (
            disabled.accepted,
            disabled.started,
            disabled.completed,
            disabled.cancelled
        ),
        (0, 0, 0, 0)
    );
    assert!(
        disabled
            .records
            .iter()
            .all(|record| matches!(record, AudioRecord::ScopeApplied { .. }))
    );
    capture.assert_silent();

    witness.enable_development_output().unwrap();
    witness.enable_development_output().unwrap();
    wait(
        || capture.drain(&witness).output_frames >= disabled.output_frames + 960,
        "the enabled original callback did not advance without historical replay",
    );
    assert_eq!(witness.snapshot().accepted, 0);
    assert!(witness.snapshot().output_enabled);
    capture.assert_silent();

    collect_near_pickup(&mut game, 20, &witness, &mut capture);
    wait(
        || capture.drain(&witness).completed == 1,
        "new enabled pickup did not complete on its original device clock",
    );
    game.command(action(TRAVEL));
    remember(&game, &mut instances);
    wait(
        || capture.drain(&witness).completed == 2,
        "new enabled travel did not complete on its original device clock",
    );
    game.close();
    capture.drain(&witness);
    let closed_observation = witness.snapshot_with_history();
    let closed = &closed_observation.snapshot;
    assert!(!closed.output_enabled);
    assert_eq!(closed.suppressed_effects, 2);
    assert!(witness.enable_development_output().is_err());
    let expected = [
        AudioCause::Pickup {
            level: LevelKey::main(LevelId::Cavern),
            entity: "gem-near".into(),
        },
        AudioCause::Travel {
            level: LevelKey::main(LevelId::Meadow),
        },
    ];
    let proof = oracle::verify(
        sources,
        &closed_observation,
        &capture.0,
        &expected,
        &instances,
    )
    .unwrap();
    assert_eq!(proof.maximum_absolute_error, 0.0);
    assert_eq!(proof.samples_compared, capture.0.len());
    assert!(proof.nonzero_samples > 0);
    assert_eq!((proof.accepted_pickups, proof.accepted_travels), (1, 1));
    let original = serde_json::to_value(&closed_observation).unwrap();

    let mut fresh = Game::from_project_with_recipe(project.root(), recipe());
    let fresh_witness = observation(&fresh);
    assert_ne!(fresh_witness.snapshot().runtime, closed.runtime);
    assert!(!fresh_witness.snapshot().output_enabled);
    crate::request_resume(&mut fresh.session, &saved).unwrap();
    fresh.settle();
    assert_eq!(
        encode_resume(&fresh.capture()).unwrap(),
        encode_resume(&saved).unwrap()
    );
    let mut fresh_capture = Capture::default();
    wait(
        || {
            let snapshot = fresh_capture.drain(&fresh_witness);
            !fresh_capture.0.is_empty() && snapshot.desired_instance == snapshot.applied_instance
        },
        "the reconstructed original callback did not apply its fresh scope",
    );
    let reconstructed = fresh_capture.drain(&fresh_witness);
    assert!(reconstructed.reset_on_rebuild && !reconstructed.output_enabled);
    assert_eq!(
        (
            reconstructed.accepted,
            reconstructed.started,
            reconstructed.completed
        ),
        (0, 0, 0)
    );
    fresh_capture.assert_silent();
    fresh_witness.enable_development_output().unwrap();
    wait(
        || fresh_capture.drain(&fresh_witness).output_frames >= reconstructed.output_frames + 960,
        "fresh enablement did not advance its independent callback clock",
    );
    assert_eq!(fresh_witness.snapshot().accepted, 0);
    fresh.close();
    let retired = fresh_capture.drain(&fresh_witness);
    assert!(retired.physical_complete && retired.stream_drop_returned);
    assert!(retired.data_callback_released && retired.error_callback_released);
    assert_eq!(fresh_capture.0.len() as u64, retired.output_frames * 2);
    fresh_capture.assert_silent();
    assert_eq!(
        original,
        serde_json::to_value(witness.snapshot_with_history()).unwrap()
    );
}

#[derive(Default)]
struct UnknownService;

impl Plugin for UnknownService {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test.unknown-audio-service"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[test]
#[ignore = "requires an actual Windows stereo 48 kHz f32 output device; run serially"]
fn ordinary_audio_capture_and_unknown_services_still_refuse_reconstruction() {
    let project = ProjectCopy::new();
    let mut ordinary = Game::from_project_with_recipe(
        project.root(),
        audio::recipe(RunOptions::default(), AudioOptions::default()),
    );
    let witness = observation(&ordinary);
    assert!(!witness.snapshot().reset_on_rebuild);
    assert!(witness.snapshot().output_enabled);
    assert!(witness.enable_development_output().is_err());
    assert!(matches!(
        crate::capture_resume(&ordinary.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains(audio::AUDIO_PLUGIN_ID.as_str())
    ));
    ordinary.close();
    assert!(witness.snapshot().physical_complete);

    let recipe = audio::development_recipe(RunOptions::default(), AudioOptions::default())
        .add_plugin::<UnknownService>()
        .unwrap();
    let mut unknown = Game::from_project_with_recipe(project.root(), recipe);
    let unknown_witness = observation(&unknown);
    assert!(matches!(
        crate::capture_resume(&unknown.session),
        Err(GameResumeError::Ineligible(reason))
            if reason.contains("two-level-game.test.unknown-audio-service")
                && !reason.contains(audio::AUDIO_PLUGIN_ID.as_str())
    ));
    unknown.close();
    assert!(unknown_witness.snapshot().physical_complete);
}

#[test]
#[ignore = "requires an actual Windows stereo 48 kHz f32 output device; run serially"]
fn a_development_startup_failure_keeps_output_disabled_and_retires_the_original_device() {
    let captured = Arc::new(Mutex::new(None::<AudioWitness>));
    let observe = captured.clone();
    let mut app = App::new();
    let mut plugin = isolated_audio(None);
    plugin.policy = AudioPolicy::ResetOnRebuild;
    app.add_plugin(plugin).unwrap();
    app.add_systems(
        StartupStage::Tooling,
        (move |world: &mut World| -> Result<(), BevyError> {
            *observe.lock().unwrap() = audio::witness(world);
            Err(BevyError::error(std::io::Error::other(
                "intentional development failure after audio startup",
            )))
        })
        .after(super::start_audio),
    )
    .unwrap();
    let mut failure = RuntimeInstance::start(app).expect_err("the later startup system must fail");
    let witness = captured
        .lock()
        .unwrap()
        .clone()
        .expect("original CPAL owner must have opened");
    let deadline = Instant::now() + Duration::from_secs(10);
    while failure.drive_retirement() != RuntimeCandidateRetirementState::Retired {
        if Instant::now() >= deadline {
            drop(failure);
            panic!("failed development startup did not complete original native retirement");
        }
        std::thread::yield_now();
    }
    let snapshot = witness.snapshot();
    assert!(snapshot.reset_on_rebuild && !snapshot.output_enabled);
    assert!(snapshot.play_admitted);
    assert_eq!(snapshot.accepted, 0);
    assert_eq!(snapshot.suppressed_effects, 0);
    assert_eq!(snapshot.faults, 0);
    assert!(snapshot.error.is_none());
    assert!(snapshot.physical_complete && snapshot.stream_drop_returned);
    assert!(snapshot.data_callback_released && snapshot.error_callback_released);
    assert!(witness.enable_development_output().is_err());
}
