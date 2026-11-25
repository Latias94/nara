use std::time::Duration;

use bevy_ecs::prelude::{Entity, On, Query, ResMut, Resource, Schedule, World};
use nara_asset::{Assets, Handle, allocate_anonymous_handle};
use nara_audio::{
    AudioBusId, AudioBusMeter, AudioClip, AudioClipError, AudioError, AudioFinished, AudioListener,
    AudioMixer, AudioOutput, AudioPlayback, AudioPlayer, AudioPosition2d, PlaybackState,
    SpatialAudio, setup_audio, update_audio_players,
};
use nara_core::Vec2;

#[test]
fn autoplay_waits_for_asset_and_notifies_before_despawn() {
    let (mut world, mut schedule) = runtime();
    let clip = allocate_anonymous_handle().unwrap();
    let entity = world
        .spawn(AudioPlayer {
            despawn_on_finish: true,
            ..AudioPlayer::new(clip)
        })
        .id();

    advance(&mut world, &mut schedule, 100);
    assert_eq!(state(&world, entity), PlaybackState::WaitingForClip);
    world
        .resource_mut::<Assets<AudioClip>>()
        .insert(clip, tone(40));
    schedule.run(&mut world);
    assert_eq!(state(&world, entity), PlaybackState::Playing);

    advance(&mut world, &mut schedule, 100);
    assert!(world.get_entity(entity).is_err());
    advance(&mut world, &mut schedule, 100);
    assert_eq!(
        world.resource::<Finished>().0,
        [(entity, clip, PlaybackState::Finished)]
    );
}

#[test]
fn looping_player_can_pause_resume_stop_and_release_its_voice() {
    let (mut world, mut schedule) = runtime();
    let clip = add_tone(&mut world, 20);
    let player = AudioPlayer {
        looping: true,
        autoplay: false,
        despawn_on_finish: true,
        ..AudioPlayer::new(clip)
    };
    let entity = world.spawn(player.clone()).id();
    advance(&mut world, &mut schedule, 100);
    assert_eq!(state(&world, entity), PlaybackState::Idle);

    world.get_mut::<AudioPlayback>(entity).unwrap().play();
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 100);
    assert_eq!(state(&world, entity), PlaybackState::Playing);
    assert_level(meter(&world, &AudioBusId::SFX).peak_left, 0.5);

    world.get_mut::<AudioPlayback>(entity).unwrap().pause();
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 20);
    assert_eq!(state(&world, entity), PlaybackState::Paused);
    assert_silent(meter(&world, &AudioBusId::SFX));

    world.get_mut::<AudioPlayback>(entity).unwrap().resume();
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 20);
    assert_eq!(state(&world, entity), PlaybackState::Playing);
    assert_level(meter(&world, &AudioBusId::SFX).peak_left, 0.5);

    world.get_mut::<AudioPlayback>(entity).unwrap().pause();
    schedule.run(&mut world);
    world.get_mut::<AudioPlayback>(entity).unwrap().resume();
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 20);
    assert_level(meter(&world, &AudioBusId::SFX).peak_left, 0.5);

    world.get_mut::<AudioPlayback>(entity).unwrap().stop();
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 20);
    assert_eq!(state(&world, entity), PlaybackState::Stopped);
    assert_silent(meter(&world, &AudioBusId::SFX));
    assert!(world.resource::<Finished>().0.is_empty());

    world.get_mut::<AudioPlayback>(entity).unwrap().play();
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 30);
    assert_level(meter(&world, &AudioBusId::SFX).peak_left, 0.5);
    world
        .entity_mut(entity)
        .remove::<AudioPlayer>()
        .insert(player);
    advance(&mut world, &mut schedule, 30);
    assert_eq!(state(&world, entity), PlaybackState::Idle);
    assert_silent(meter(&world, &AudioBusId::SFX));
    assert!(world.resource::<Finished>().0.is_empty());
}

#[test]
fn one_shots_measure_real_bus_output_through_mute_solo_volume_and_fade() {
    let mut mixer = AudioMixer::mock().unwrap();
    let music = AudioBusId::new(String::from("music"));
    mixer.add_bus(music.clone()).unwrap();
    let clip = tone(600);
    let music_playback = mixer.play_one_shot(&clip, &music).unwrap();
    drop(mixer.play_one_shot(&clip, &AudioBusId::SFX).unwrap());

    mixer.advance_mock(Duration::from_millis(20));
    assert_eq!(music_playback.state(), PlaybackState::Playing);
    assert_level(mixer.meter_snapshot()[&AudioBusId::MASTER].peak_left, 1.0);

    mixer.set_bus_volume(&music, 0.5).unwrap();
    mixer.set_bus_soloed(&music, true).unwrap();
    mixer.advance_mock(Duration::from_millis(20));
    let meters = mixer.meter_snapshot();
    assert_silent(meters[&AudioBusId::SFX]);
    assert_level(meters[&music].peak_left, 0.25);
    assert_level(meters[&AudioBusId::MASTER].peak_left, 0.25);

    mixer.set_bus_muted(&music, true).unwrap();
    mixer.advance_mock(Duration::from_millis(20));
    let meters = mixer.meter_snapshot();
    assert_silent(meters[&music]);
    assert_silent(meters[&AudioBusId::MASTER]);

    mixer.set_bus_muted(&music, false).unwrap();
    mixer.advance_mock(Duration::from_millis(20));
    mixer
        .fade_bus_volume(&music, 1.0, Duration::from_millis(100))
        .unwrap();
    mixer.advance_mock(Duration::from_millis(50));
    let halfway = mixer.meter_snapshot()[&music].peak_left;
    assert!(halfway > 0.25 && halfway < 0.5, "peak was {halfway}");

    mixer.set_bus_volume(&AudioBusId::MASTER, 0.5).unwrap();
    mixer.advance_mock(Duration::from_millis(70));
    let meters = mixer.meter_snapshot();
    assert_level(meters[&music].peak_left, 0.5);
    assert_level(meters[&AudioBusId::MASTER].peak_left, 0.25);
    assert_eq!(mixer.bus_settings(&music).unwrap().volume, 1.0);

    mixer.advance_mock(Duration::from_millis(600));
    assert_eq!(music_playback.state(), PlaybackState::Finished);
    assert_silent(mixer.meter_snapshot()[&AudioBusId::MASTER]);
}

#[test]
fn mute_interrupts_an_in_progress_fade_to_silence() {
    let mut mixer = AudioMixer::mock().unwrap();
    mixer.play_one_shot(&tone(500), &AudioBusId::SFX).unwrap();
    mixer.advance_mock(Duration::from_millis(20));
    mixer
        .fade_bus_volume(&AudioBusId::SFX, 0.0, Duration::from_secs(1))
        .unwrap();
    mixer.advance_mock(Duration::from_millis(20));
    assert!(mixer.meter_snapshot()[&AudioBusId::SFX].peak_left > 0.0);

    mixer.set_bus_muted(&AudioBusId::SFX, true).unwrap();
    mixer.advance_mock(Duration::from_millis(20));
    assert_silent(mixer.meter_snapshot()[&AudioBusId::SFX]);
}

#[test]
fn moving_listener_and_emitter_change_stereo_levels() {
    let (mut world, mut schedule) = runtime();
    let clip = add_tone(&mut world, 200);
    let emitter = world
        .spawn((
            AudioPlayer::new(clip),
            SpatialAudio {
                min_distance: 2.0,
                max_distance: 10.0,
            },
        ))
        .id();
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 20);
    assert_silent(meter(&world, &AudioBusId::SFX));

    let listener = world.spawn(AudioListener).id();
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 20);
    let near = meter(&world, &AudioBusId::SFX);
    assert_level(near.peak_left, 0.5);
    assert_level(near.peak_right, 0.5);

    world.get_mut::<AudioPosition2d>(listener).unwrap().0 = Vec2::new(-6.0, 0.0);
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 20);
    let farther = meter(&world, &AudioBusId::SFX);
    assert!(farther.peak_left > 0.0 && farther.peak_left < farther.peak_right);
    assert!(farther.peak_right < near.peak_right);

    world.get_mut::<AudioPosition2d>(emitter).unwrap().0 = Vec2::new(12.0, 0.0);
    schedule.run(&mut world);
    advance(&mut world, &mut schedule, 20);
    assert_silent(meter(&world, &AudioBusId::SFX));
    assert_eq!(state(&world, emitter), PlaybackState::Playing);
}

#[test]
fn mock_accumulates_fractional_samples() {
    let mut mixer = AudioMixer::mock().unwrap();
    assert_eq!(mixer.output(), AudioOutput::Mock);
    let sound = mixer.play_one_shot(&tone(20), &AudioBusId::SFX).unwrap();
    for _ in 0..2 {
        mixer.advance_mock(Duration::from_micros(10));
    }
    assert_silent(mixer.meter_snapshot()[&AudioBusId::SFX]);
    mixer.advance_mock(Duration::from_micros(10));
    assert_level(mixer.meter_snapshot()[&AudioBusId::SFX].peak_left, 0.5);
    assert_eq!(sound.state(), PlaybackState::Playing);
}

#[test]
fn rejects_invalid_clip_and_playback_settings() {
    assert!(matches!(
        AudioClip::from_bytes(b"not an audio file"),
        Err(AudioClipError::Decode(_))
    ));
    let (mut world, mut schedule) = runtime();
    let clip = add_tone(&mut world, 20);
    let entity = world
        .spawn(AudioPlayer {
            pitch: f32::NAN,
            ..AudioPlayer::new(clip)
        })
        .id();
    schedule.run(&mut world);
    let playback = world.get::<AudioPlayback>(entity).unwrap();
    assert_eq!(playback.state(), PlaybackState::Failed);
    assert!(matches!(playback.error(), Some(AudioError::InvalidPitch)));
}

#[cfg(not(feature = "device"))]
#[test]
fn default_output_is_mock_when_device_support_is_disabled() {
    let mixer = AudioMixer::new().unwrap();
    assert_eq!(mixer.output(), AudioOutput::Mock);
    assert!(mixer.device_error().is_none());
}

#[derive(Default, Resource)]
struct Finished(Vec<(Entity, Handle<AudioClip>, PlaybackState)>);

fn runtime() -> (World, Schedule) {
    let mut world = World::new();
    setup_audio(&mut world, AudioMixer::mock().unwrap());
    world.init_resource::<Finished>();
    world.add_observer(
        |event: On<AudioFinished>,
         players: Query<&AudioPlayback>,
         mut finished: ResMut<Finished>| {
            finished.0.push((
                event.entity,
                event.clip,
                players.get(event.entity).unwrap().state(),
            ));
        },
    );
    let mut schedule = Schedule::default();
    schedule.add_systems(update_audio_players);
    (world, schedule)
}

fn advance(world: &mut World, schedule: &mut Schedule, millis: u64) {
    world
        .resource_mut::<AudioMixer>()
        .advance_mock(Duration::from_millis(millis));
    schedule.run(world);
}

fn state(world: &World, entity: Entity) -> PlaybackState {
    world.get::<AudioPlayback>(entity).unwrap().state()
}

fn meter(world: &World, bus: &AudioBusId) -> AudioBusMeter {
    world.resource::<AudioMixer>().meter_snapshot()[bus]
}

fn assert_level(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

fn assert_silent(meter: AudioBusMeter) {
    assert_eq!(meter, AudioBusMeter::default());
}

fn add_tone(world: &mut World, millis: u32) -> Handle<AudioClip> {
    let handle = allocate_anonymous_handle().unwrap();
    world
        .resource_mut::<Assets<AudioClip>>()
        .insert(handle, tone(millis));
    handle
}

fn tone(millis: u32) -> AudioClip {
    let frames = millis * 48;
    let data_bytes = frames * 2;
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
    wav.extend_from_slice(&48_000_u32.to_le_bytes());
    wav.extend_from_slice(&96_000_u32.to_le_bytes());
    wav.extend_from_slice(b"\x02\0\x10\0data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for _ in 0..frames {
        wav.extend_from_slice(&16_384_i16.to_le_bytes());
    }
    let clip = AudioClip::from_bytes(wav).unwrap();
    assert_eq!(clip.duration(), Duration::from_millis(u64::from(millis)));
    clip
}
