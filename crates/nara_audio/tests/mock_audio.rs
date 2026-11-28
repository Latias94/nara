use std::time::{Duration, Instant};

use bevy_ecs::{
    change_detection::DetectChanges,
    entity_disabling::Disabled,
    prelude::{Entity, Mut, On, Query, ResMut, Resource, Schedule, World},
};
use nara_app::App;
use nara_asset::{
    AssetAppExt, AssetEvents, AssetPlugin, AssetReferenceDemand, AssetStates, Assets, Handle,
    NpakEntryKind, RuntimeAssetMode, RuntimeAssetSource, allocate_anonymous_handle,
    load_asset_sync, source::asset_id_path,
};
use nara_audio::{
    AudioBusId, AudioBusMeter, AudioClip, AudioClipError, AudioClipSource, AudioError,
    AudioFinished, AudioListener, AudioMixer, AudioOutput, AudioPlayback, AudioPlayer, AudioPlugin,
    AudioPosition2d, PlaybackState, SpatialAudio, cook_audio, setup_audio, update_audio_players,
};
use nara_core::{AssetId, Vec2, vfs::MemoryAssetSource};
use nara_reflect::{ComponentValue, ComponentValueShape, Schema};

const CLIP_ID: &str = "0193a100-1111-7000-8000-000000000001";
const STEP: Duration = Duration::from_millis(20);

#[test]
fn authored_sources_hydrate_and_use_the_existing_transport() {
    let clip_id = CLIP_ID.parse().unwrap();
    let mut app = App::new();
    app.add_plugin(AssetPlugin).unwrap();
    app.add_plugin(AudioPlugin::mock()).unwrap();
    let cooked = cook_audio(&tone_wav(200)).unwrap();
    install_clip_source(&mut app, clip_id, RuntimeAssetMode::Cooked, &cooked);

    let schema = AudioClipSource::type_schema().unwrap();
    assert_eq!(schema.id().as_str(), "nara.AudioClipSource");
    let clip_field = schema
        .fields()
        .iter()
        .find(|field| field.id().as_str() == "clip")
        .unwrap();
    assert_eq!(
        clip_field.shape(),
        &ComponentValueShape::stable_asset_ref(AssetReferenceDemand::Immediate)
    );
    let component = ComponentValue::map([("clip", ComponentValue::AssetReference(clip_id))]);
    let authored = AudioClipSource::decode_component((&component).into()).unwrap();
    assert_eq!(authored.bus, "sfx");
    assert_eq!(authored.volume, 1.0);
    assert!(authored.autoplay);
    assert_eq!(
        AudioClipSource::decode_postcard(&authored.encode_postcard().unwrap()).unwrap(),
        authored
    );

    let world = app.world_mut().unwrap();
    world.spawn(AudioListener);
    let autoplay = world
        .spawn((
            AudioClipSource {
                bus: "bgm".to_owned(),
                volume: 0.75,
                looping: true,
                spatial: true,
                ..authored.clone()
            },
            AudioPosition2d(Vec2::new(5.0, 0.0)),
        ))
        .id();
    let manual = world
        .spawn(AudioClipSource {
            autoplay: false,
            ..authored
        })
        .id();
    assert_eq!(state(world, manual), PlaybackState::Idle);

    app.step_fixed(STEP).unwrap();
    let player = app.world().get::<AudioPlayer>(autoplay).unwrap();
    assert_eq!(player.bus, AudioBusId::BGM);
    assert_eq!(player.volume, 0.75);
    assert!(player.looping);
    assert!(app.world().get::<SpatialAudio>(autoplay).is_some());
    assert!(app.world().get::<SpatialAudio>(manual).is_none());
    assert_eq!(state(app.world(), autoplay), PlaybackState::WaitingForClip);
    assert_eq!(state(app.world(), manual), PlaybackState::Idle);

    let handle = player.clip;
    assert_eq!(
        load_asset_sync::<AudioClip>(app.world_mut().unwrap(), clip_id).unwrap(),
        handle
    );
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), autoplay), PlaybackState::Playing);
    assert_eq!(state(app.world(), manual), PlaybackState::Idle);
    app.world_mut()
        .unwrap()
        .get_mut::<AudioPlayback>(manual)
        .unwrap()
        .play();
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), manual), PlaybackState::Playing);
    let bgm = meter(app.world(), &AudioBusId::BGM);
    assert!(bgm.peak_left > 0.0 && bgm.peak_right > bgm.peak_left);

    app.world_mut()
        .unwrap()
        .entity_mut(autoplay)
        .remove::<AudioClipSource>();
    app.step_fixed(STEP).unwrap();
    assert!(app.world().get::<AudioPlayer>(autoplay).is_none());
    assert!(app.world().get::<SpatialAudio>(autoplay).is_none());
    assert_silent(meter(app.world(), &AudioBusId::BGM));
}

#[test]
fn authored_autoplay_recovers_from_request_and_decode_failures_after_remounts() {
    let clip_id = CLIP_ID.parse().unwrap();
    let mut app = App::new();
    app.add_plugin(AssetPlugin).unwrap();
    app.add_plugin(AudioPlugin::mock()).unwrap();
    app.set_asset_source(RuntimeAssetSource::from_memory_with_kinds(
        RuntimeAssetMode::Cooked,
        MemoryAssetSource::new(),
        [(clip_id, NpakEntryKind::Raw)],
    ))
    .unwrap();
    let source = AudioClipSource {
        clip: clip_id,
        bus: "sfx".to_owned(),
        volume: 1.0,
        looping: true,
        autoplay: true,
        spatial: false,
    };
    let entity = app.world_mut().unwrap().spawn(source.clone()).id();

    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Failed);
    assert!(app.world().get::<AudioPlayer>(entity).is_none());
    let failure_tick = app
        .world()
        .entity(entity)
        .get_ref::<AudioPlayback>()
        .unwrap()
        .last_changed();
    for _ in 0..3 {
        app.step_fixed(STEP).unwrap();
    }
    assert_eq!(
        app.world()
            .entity(entity)
            .get_ref::<AudioPlayback>()
            .unwrap()
            .last_changed(),
        failure_tick
    );

    let cooked = cook_audio(&tone_wav(200)).unwrap();
    app.world_mut().unwrap().entity_mut(entity).insert(Disabled);
    install_clip_source(
        &mut app,
        clip_id,
        RuntimeAssetMode::Cooked,
        &cooked[..cooked.len() - 1],
    );
    app.step_fixed(STEP).unwrap();
    app.world_mut()
        .unwrap()
        .entity_mut(entity)
        .remove::<Disabled>();
    let stopped = app.world_mut().unwrap().spawn(source).id();
    app.step_fixed(STEP).unwrap();
    let handle = app.world().get::<AudioPlayer>(entity).unwrap().clip;
    assert_eq!(state(app.world(), entity), PlaybackState::WaitingForClip);
    assert!(load_asset_sync::<AudioClip>(app.world_mut().unwrap(), clip_id).is_err());
    app.step_fixed(STEP).unwrap();
    let playback = app.world().get::<AudioPlayback>(entity).unwrap();
    assert_eq!(playback.state(), PlaybackState::Failed);
    assert!(matches!(
        playback.error(),
        Some(AudioError::ClipLoadFailed(_))
    ));

    install_clip_source(&mut app, clip_id, RuntimeAssetMode::Cooked, &cooked);
    app.world_mut()
        .unwrap()
        .get_mut::<AudioPlayback>(stopped)
        .unwrap()
        .stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while state(app.world(), entity) != PlaybackState::Playing {
        assert!(Instant::now() < deadline, "the remounted clip did not play");
        app.update_with_delta(Duration::ZERO).unwrap();
        app.step_fixed(STEP).unwrap();
        std::thread::yield_now();
    }
    assert_eq!(app.world().get::<AudioPlayer>(entity).unwrap().clip, handle);
    assert_eq!(state(app.world(), stopped), PlaybackState::Stopped);
}

#[test]
fn authored_autoplay_retries_corrected_settings_and_respects_manual_transport() {
    let clip_id = CLIP_ID.parse().unwrap();
    let mut app = App::new();
    app.add_plugins((AssetPlugin, AudioPlugin::mock())).unwrap();
    install_clip_source(&mut app, clip_id, RuntimeAssetMode::Source, &tone_wav(200));
    let world = app.world_mut().unwrap();
    load_asset_sync::<AudioClip>(world, clip_id).unwrap();
    let entity = world
        .spawn(AudioClipSource {
            clip: clip_id,
            bus: "missing-bus".to_owned(),
            volume: 1.0,
            looping: true,
            autoplay: true,
            spatial: false,
        })
        .id();

    app.step_fixed(STEP).unwrap();
    assert!(matches!(
        app.world().get::<AudioPlayback>(entity).unwrap().error(),
        Some(AudioError::UnknownBus(_))
    ));
    {
        let mut source = app
            .world_mut()
            .unwrap()
            .get_mut::<AudioClipSource>(entity)
            .unwrap();
        source.bus = "bgm".to_owned();
        source.volume = 3.0;
    }
    app.step_fixed(STEP).unwrap();
    app.step_fixed(STEP).unwrap();
    assert!(matches!(
        app.world().get::<AudioPlayback>(entity).unwrap().error(),
        Some(AudioError::InvalidVolume)
    ));

    {
        let mut source = app
            .world_mut()
            .unwrap()
            .get_mut::<AudioClipSource>(entity)
            .unwrap();
        source.volume = 0.75;
        source.autoplay = false;
    }
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Failed);

    let world = app.world_mut().unwrap();
    world.get_mut::<AudioClipSource>(entity).unwrap().autoplay = true;
    app.step_fixed(STEP).unwrap();
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Playing);

    let world = app.world_mut().unwrap();
    world.get_mut::<AudioPlayback>(entity).unwrap().pause();
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Paused);
    app.world_mut()
        .unwrap()
        .get_mut::<AudioClipSource>(entity)
        .unwrap()
        .volume = 3.0;
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Failed);
    app.world_mut()
        .unwrap()
        .get_mut::<AudioClipSource>(entity)
        .unwrap()
        .volume = 0.75;
    app.step_fixed(STEP).unwrap();
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Paused);
    app.world_mut()
        .unwrap()
        .get_mut::<AudioPlayback>(entity)
        .unwrap()
        .resume();
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Playing);

    let world = app.world_mut().unwrap();
    world.get_mut::<AudioPlayback>(entity).unwrap().stop();
    world.get_mut::<AudioClipSource>(entity).unwrap().bus = "sfx".to_owned();
    app.step_fixed(STEP).unwrap();
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Stopped);

    let world = app.world_mut().unwrap();
    let handle = world.get::<AudioPlayer>(entity).unwrap().clip;
    world.resource_scope(|world, mut clips: Mut<Assets<AudioClip>>| {
        world.resource_scope(|world, mut states: Mut<AssetStates>| {
            clips
                .remove_with_state(
                    handle,
                    &mut states,
                    &mut world.resource_mut::<AssetEvents>(),
                )
                .unwrap();
        });
    });
    world.get_mut::<AudioClipSource>(entity).unwrap().volume = 0.5;
    world.get_mut::<AudioPlayback>(entity).unwrap().play();
    let deadline = Instant::now() + Duration::from_secs(5);
    while state(app.world(), entity) != PlaybackState::Playing {
        assert!(
            Instant::now() < deadline,
            "the retired clip did not reload after play"
        );
        app.step_fixed(STEP).unwrap();
        app.update_with_delta(Duration::ZERO).unwrap();
        std::thread::yield_now();
    }
}

#[test]
fn audio_plugin_loads_source_clips_when_asset_plugin_is_installed_later() {
    let clip_id = CLIP_ID.parse().unwrap();
    let mut app = App::new();
    app.add_plugin(AudioPlugin::mock()).unwrap();
    app.add_plugin(AssetPlugin).unwrap();
    install_clip_source(&mut app, clip_id, RuntimeAssetMode::Source, &tone_wav(40));
    app.update_with_delta(Duration::ZERO).unwrap();
    let clip = load_asset_sync::<AudioClip>(app.world_mut().unwrap(), clip_id).unwrap();
    let loaded = app
        .world()
        .resource::<Assets<AudioClip>>()
        .get(clip)
        .unwrap();
    assert_eq!(loaded.duration(), Duration::from_millis(40));
}

#[cfg(feature = "scene-runtime")]
#[test]
fn audio_plugin_registers_clip_source_in_both_scene_plugin_orders() {
    use nara_reflect::{ComponentRegistryPlugin, component_registry};
    use nara_scene::ScenePlugin;

    let registered = |app: &App| {
        component_registry(app.world())
            .unwrap()
            .components()
            .iter()
            .any(|schema| schema.id().as_str() == AudioClipSource::COMPONENT_ID)
    };

    let mut scene_first = App::new();
    scene_first
        .add_plugins((ComponentRegistryPlugin, AssetPlugin, ScenePlugin))
        .unwrap();
    scene_first.add_plugin(AudioPlugin::mock()).unwrap();
    assert!(registered(&scene_first));
    scene_first.update_with_delta(Duration::ZERO).unwrap();

    let mut audio_first = App::new();
    audio_first.add_plugin(AudioPlugin::mock()).unwrap();
    audio_first
        .add_plugins((ComponentRegistryPlugin, AssetPlugin, ScenePlugin))
        .unwrap();
    audio_first.update_with_delta(Duration::ZERO).unwrap();
    assert!(registered(&audio_first));
}

#[test]
fn standalone_audio_plugin_plays_procedural_clips() {
    let mut app = App::new();
    app.add_plugin(AudioPlugin::mock()).unwrap();
    let world = app.world_mut().unwrap();
    let clip = add_tone(world, 40);
    let entity = world.spawn(AudioPlayer::new(clip)).id();
    app.step_fixed(STEP).unwrap();
    assert_eq!(state(app.world(), entity), PlaybackState::Playing);
}

fn install_clip_source(app: &mut App, id: AssetId, mode: RuntimeAssetMode, bytes: &[u8]) {
    let mut source = MemoryAssetSource::new();
    source.insert(&asset_id_path(id), bytes).unwrap();
    app.set_asset_source(RuntimeAssetSource::from_memory(mode, source))
        .unwrap();
}

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
    let clip = AudioClip::from_bytes(tone_wav(millis)).unwrap();
    assert_eq!(clip.duration(), Duration::from_millis(u64::from(millis)));
    clip
}

fn tone_wav(millis: u32) -> Vec<u8> {
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
    wav
}
