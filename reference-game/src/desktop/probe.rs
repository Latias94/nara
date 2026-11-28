use std::{
    error::Error,
    path::Path,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use nara::{
    app::FrameExecutionStart,
    audio::{AudioBusId, AudioOutput, PlaybackState},
    ecs::{Local, Single, schedule::IntoScheduleConfigs},
    prelude::*,
    render_wgpu::desktop::{DesktopPresentation, DesktopPresentedFrame},
    save::SaveStore,
};

use crate::{
    PilotInput, Round, RoundStatus, RunRules,
    audio::ARRIVAL_BUS,
    campaign::{ARRIVAL_AUDIO, Campaign, SaveSnapshot, Sector},
    route::{ESCAPE_ROUTE, at},
    simulation::advance_round,
};

use super::is_game_frame;

pub(super) const SAVE_SLOT: &str = "r1-verification";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Smoke {
    FirstPresent,
    Pickup,
}

pub(super) fn verify_present(app: &mut App, probe: Smoke) -> Result<(), Box<dyn Error>> {
    let require_pickup = probe == Smoke::Pickup;
    if require_pickup {
        install_route(app, true)?;
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let presented = Arc::new(OnceLock::new());
    let receipt = Arc::clone(&presented);
    app.add_systems(
        Last,
        move |observation: Res<DesktopPresentation>,
              execution: Res<FrameExecutionStart>,
              round: Res<Round>,
              mut pickup_frame: Local<Option<u64>>,
              mut exit: ResMut<AppExitRequests>| {
            if round.collected() > 0 && pickup_frame.is_none() {
                *pickup_frame = Some(execution.frame());
            }
            if Instant::now() >= deadline {
                exit.request_exit();
            } else if let Some(frame) = observation.latest().filter(is_game_frame)
                && (!require_pickup || pickup_frame.is_some_and(|index| frame.frame_index >= index))
            {
                let _ = receipt.set(frame);
                exit.request_success();
            }
        },
    )?;
    let exit = app.run()?;
    if let (AppExit::Success, Some(frame)) = (exit, presented.get()) {
        println!(
            "Night Relay presented {}x{} with {} HUD glyphs",
            frame.extent.width, frame.extent.height, frame.hud_glyph_primitives,
        );
        if require_pickup {
            println!("Night Relay collected a power cell and presented the updated HUD");
        }
        return Ok(());
    }
    Err(if require_pickup {
        "the game closed or timed out before picking up a cell and presenting the HUD"
    } else {
        "the window closed or timed out before presenting the game and HUD"
    }
    .into())
}

fn install_route(app: &mut App, pickup_only: bool) -> Result<(), PluginError> {
    let route = if pickup_only {
        &ESCAPE_ROUTE[..4]
    } else {
        &ESCAPE_ROUTE[..]
    };
    app.add_systems(
        FixedUpdate,
        (move |mut waypoint: Local<usize>,
               mut input: ResMut<PilotInput>,
               round: Res<Round>,
               campaign: Res<Campaign>,
               rules: Single<&RunRules>,
               time: Res<FixedTime>| {
            input.movement = Vec2::ZERO;
            if campaign.sector != Sector::One
                || round.status() != RoundStatus::Running
                || pickup_only && round.collected() != 0
            {
                return;
            }
            while let Some(&(column, row)) = route.get(*waypoint) {
                let offset = at(column, row) - round.position();
                if offset.length_squared() > 0.01 {
                    input.movement = offset / (rules.move_speed * time.delta().as_secs_f32());
                    return;
                }
                *waypoint += 1;
            }
        })
        .in_set(FixedUpdateSet::Simulate)
        .before(advance_round),
    )?;
    Ok(())
}

struct R1Receipt {
    retired: usize,
    frame: DesktopPresentedFrame,
    output: AudioOutput,
    peak: f32,
    progress: SaveSnapshot,
}

pub(super) fn verify_r1(app: &mut App, save_dir: &Path) -> Result<(), Box<dyn Error>> {
    install_route(app, false)?;
    let manager = app.world().resource::<SceneManager>();
    let first = manager
        .main_instance()
        .ok_or("Sector 1 was not installed")?;
    let authored = manager
        .entities(first)
        .ok_or("Sector 1 is not ready")?
        .len();
    let arrival: AssetId = ARRIVAL_AUDIO.parse()?;
    let bus = AudioBusId::new(ARRIVAL_BUS);
    let deadline = Instant::now() + Duration::from_secs(30);
    let result = Arc::new(OnceLock::<Result<R1Receipt, String>>::new());
    let receipt = Arc::clone(&result);
    app.add_systems(
        Last,
        move |(present, execution, mixer): (
            Res<DesktopPresentation>,
            Res<FrameExecutionStart>,
            Res<AudioMixer>,
        ),
              (manager, campaign, round): (Res<SceneManager>, Res<Campaign>, Res<Round>),
              entities: Query<(Entity, Option<&SceneMember>)>,
              audio: Query<(&SceneMember, &AudioClipSource, &AudioPlayback)>,
              mut old: Local<Vec<Entity>>,
              mut switched_at: Local<Option<u64>>,
              mut playing: Local<bool>,
              mut peak: Local<f32>,
              mut exit: ResMut<AppExitRequests>| {
            let mut fail = |message| {
                let _ = receipt.set(Err(message));
                exit.request_exit();
            };
            if old.is_empty() {
                old.extend(entities.iter().filter_map(|(entity, member)| {
                    (member == Some(&SceneMember(first))).then_some(entity)
                }));
                if old.len() <= authored {
                    fail("the first arena is missing its scene-owned decorations".into());
                    return;
                }
            }
            if matches!(round.status(), RoundStatus::Failed(_)) {
                fail(format!(
                    "the verification route ended in {:?}",
                    round.status()
                ));
                return;
            }
            if Instant::now() >= deadline {
                fail(format!(
                    "timed out waiting for Sector 2, arrival playback and presentation \
                     (sector={:?}, round={:?}, audio_output={:?}, playing={}, arrival_peak={})",
                    campaign.sector,
                    round.status(),
                    mixer.output(),
                    *playing,
                    *peak,
                ));
                return;
            }
            if campaign.sector != Sector::Two {
                if round.status() == RoundStatus::Escaped && campaign.pending.is_none() {
                    fail("the automatic switch to Sector 2 failed".into());
                }
                return;
            }
            let Some(main) = manager.main_instance() else {
                fail("there is no active scene after extraction".into());
                return;
            };
            if main == first
                || manager.state(main) != Some(&SceneLoadState::Ready)
                || manager.state(first).is_some()
                || old.iter().any(|&entity| entities.contains(entity))
                || entities
                    .iter()
                    .any(|(_, member)| member == Some(&SceneMember(first)))
            {
                fail("Sector 1 entities or decorations survived the switch".into());
                return;
            }
            if round.status() != RoundStatus::Running || round.collected() != 0 {
                fail("Sector 2 did not start a clean round".into());
                return;
            }
            let frame_index = *switched_at.get_or_insert(execution.frame());
            let Some((_, _, playback)) = audio.iter().find(|(member, clip, _)| {
                member.0 == main && clip.clip == arrival && clip.autoplay && clip.bus == ARRIVAL_BUS
            }) else {
                fail("the packaged Sector 2 scene is missing its autoplay arrival cue".into());
                return;
            };
            let output = mixer.output();
            match playback.state() {
                PlaybackState::Playing => {
                    if let Some(meter) = mixer.meter_snapshot().get(&bus) {
                        *peak = (*peak).max(meter.peak_left.max(meter.peak_right));
                    }
                    *playing = true;
                }
                PlaybackState::Failed => {
                    fail(format!("arrival playback failed: {:?}", playback.error()));
                    return;
                }
                _ => {}
            }
            if *playing
                && (output == AudioOutput::Mock || *peak > 0.0)
                && let Some(frame) = present.latest().filter(is_game_frame)
                && frame.frame_index >= frame_index
            {
                let _ = receipt.set(Ok(R1Receipt {
                    retired: old.len(),
                    frame,
                    output,
                    peak: *peak,
                    progress: campaign.progress,
                }));
                exit.request_success();
            }
        },
    )?;
    let exit = app.run()?;
    let observation = result
        .get()
        .ok_or("the window closed before the R1 verification completed")?
        .as_ref()
        .map_err(String::as_str)?;
    if exit != AppExit::Success {
        return Err("the native R1 verification did not exit successfully".into());
    }
    let saved = SaveStore::in_dir(save_dir).load::<SaveSnapshot>(SAVE_SLOT)?;
    let best = saved
        .sector_one_best_ms
        .ok_or("the save has no Sector 1 clear time")?;
    if saved != observation.progress || best == 0 || saved.sector_two_best_ms.is_some() {
        return Err("saved progress did not match the Sector 1 clear".into());
    }
    let frame = observation.frame;
    println!(
        "NIGHT_RELAY_R1 sector=2 retired={} audio=Playing audio_output={:?} arrival_peak={:.6} \
         best_clear_ms={best} save_round_trip=true presented={}x{} hud_glyphs={}",
        observation.retired,
        observation.output,
        observation.peak,
        frame.extent.width,
        frame.extent.height,
        frame.hud_glyph_primitives,
    );
    if observation.output != AudioOutput::Device {
        return Err("the native mixer used audio_output=Mock".into());
    }
    Ok(())
}
