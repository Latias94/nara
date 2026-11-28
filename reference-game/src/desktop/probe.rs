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
    route::{ESCAPE_ROUTE, SECOND_ESCAPE_ROUTE, at},
    simulation::advance_round,
};

use super::is_game_frame;

mod visuals;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CampaignRun {
    Arrival,
    BothSectors,
}

impl CampaignRun {
    pub(super) const fn save_slot(self) -> &'static str {
        match self {
            Self::Arrival => "r1-verification",
            Self::BothSectors => "r2-verification",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Smoke {
    FirstPresent,
    Pickup,
}

pub(super) fn verify_present(app: &mut App, probe: Smoke) -> Result<(), Box<dyn Error>> {
    let require_pickup = probe == Smoke::Pickup;
    if require_pickup {
        install_route(app, true, CampaignRun::Arrival)?;
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

#[derive(Default)]
struct RouteProgress {
    waypoint: usize,
    sector: Sector,
    took_impact: bool,
}

fn install_route(app: &mut App, pickup_only: bool, run: CampaignRun) -> Result<(), PluginError> {
    let first = ESCAPE_ROUTE.map(|(column, row)| at(column, row));
    let impact = [(2, 5), (8, 5), (8, 3), (11, 3)].map(|(column, row)| at(column, row));
    app.add_systems(
        FixedUpdate,
        (move |mut progress: Local<RouteProgress>,
               mut input: ResMut<PilotInput>,
               round: Res<Round>,
               campaign: Res<Campaign>,
               rules: Single<&RunRules>,
               time: Res<FixedTime>| {
            input.movement = Vec2::ZERO;
            if round.status() != RoundStatus::Running || pickup_only && round.collected() != 0 {
                return;
            }
            if campaign.sector != progress.sector {
                progress.sector = campaign.sector;
                progress.waypoint = 0;
            }
            let route: &[Vec2] = match (run, campaign.sector) {
                (CampaignRun::Arrival, Sector::One) if pickup_only => &first[..4],
                (CampaignRun::Arrival, Sector::One) => &first,
                (CampaignRun::Arrival, Sector::Two) => return,
                (CampaignRun::BothSectors, Sector::One) if !progress.took_impact => {
                    if round.shields() < rules.shield_charges {
                        if round.cooldown <= Duration::from_millis(800) {
                            progress.took_impact = true;
                            progress.waypoint = 0;
                            input.restart = true;
                        }
                        return;
                    }
                    &impact
                }
                (CampaignRun::BothSectors, Sector::One) => &first,
                (CampaignRun::BothSectors, Sector::Two) => {
                    if round.elapsed < Duration::from_millis(600) {
                        return;
                    }
                    &SECOND_ESCAPE_ROUTE
                }
            };
            while let Some(&target) = route.get(progress.waypoint) {
                let offset = target - round.position();
                if offset.length_squared() > 0.01 {
                    input.movement = offset / (rules.move_speed * time.delta().as_secs_f32());
                    return;
                }
                progress.waypoint += 1;
            }
        })
        .in_set(FixedUpdateSet::Simulate)
        .before(advance_round),
    )?;
    Ok(())
}

struct CampaignReceipt {
    retired: usize,
    frame: DesktopPresentedFrame,
    output: AudioOutput,
    peak: f32,
    progress: SaveSnapshot,
    merged_tiles: Option<(usize, usize)>,
    quiet_frames: u32,
}

pub(super) fn verify_campaign(
    app: &mut App,
    save_dir: &Path,
    run: CampaignRun,
) -> Result<(), Box<dyn Error>> {
    let complete = run == CampaignRun::BothSectors;
    install_route(app, false, run)?;
    if complete {
        app.init_resource::<visuals::Evidence>()?
            .add_systems(Last, visuals::observe)?;
    }
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
    let deadline = Instant::now() + Duration::from_secs(if complete { 50 } else { 30 });
    let result = Arc::new(OnceLock::<Result<CampaignReceipt, String>>::new());
    let receipt = Arc::clone(&result);
    app.add_systems(
        Last,
        (move |(present, execution, mixer): (
            Res<DesktopPresentation>,
            Res<FrameExecutionStart>,
            Res<AudioMixer>,
        ),
               (manager, campaign, round): (Res<SceneManager>, Res<Campaign>, Res<Round>),
               entities: Query<(Entity, Option<&SceneMember>)>,
               audio: Query<(&SceneMember, &AudioClipSource, &AudioPlayback)>,
               visuals: Option<Res<visuals::Evidence>>,
               mut old: Local<Vec<Entity>>,
               mut switched_at: Local<Option<u64>>,
               mut cleared_at: Local<Option<u64>>,
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
                    "campaign verification timed out \
                     (sector={:?}, round={:?}, position={:?}, audio_output={:?}, \
                      playing={}, arrival_peak={}, unconfirmed_visuals={:?})",
                    campaign.sector,
                    round.status(),
                    round.position(),
                    mixer.output(),
                    *playing,
                    *peak,
                    visuals.as_ref().map(|visuals| visuals.unconfirmed()),
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
            if switched_at.is_none()
                && (round.status() != RoundStatus::Running || round.collected() != 0)
            {
                fail("Sector 2 did not start a clean round".into());
                return;
            }
            let frame_index = *switched_at.get_or_insert(execution.frame());
            if complete && round.status() == RoundStatus::Escaped {
                cleared_at.get_or_insert(execution.frame());
            }
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
                && (!complete
                    || round.status() == RoundStatus::Escaped
                        && visuals.as_ref().is_some_and(|visuals| visuals.complete()))
                && let Some(frame) = present.latest().filter(is_game_frame)
                && frame.frame_index >= cleared_at.unwrap_or(frame_index)
            {
                let _ = receipt.set(Ok(CampaignReceipt {
                    retired: old.len(),
                    frame,
                    output,
                    peak: *peak,
                    progress: campaign.progress,
                    merged_tiles: visuals.as_ref().and_then(|visuals| visuals.merged_tiles),
                    quiet_frames: visuals.as_ref().map_or(0, |visuals| visuals.quiet_frames),
                }));
                exit.request_success();
            }
        })
        .after(visuals::observe),
    )?;
    let exit = app.run()?;
    let observation = result
        .get()
        .ok_or("the window closed before campaign verification completed")?
        .as_ref()
        .map_err(String::as_str)?;
    if exit != AppExit::Success {
        return Err("the native campaign verification did not exit successfully".into());
    }
    let saved = SaveStore::in_dir(save_dir).load::<SaveSnapshot>(run.save_slot())?;
    let best = saved
        .sector_one_best_ms
        .ok_or("the save has no Sector 1 clear time")?;
    if saved != observation.progress || best == 0 {
        return Err("saved progress did not match the campaign clear times".into());
    }
    let frame = observation.frame;
    if complete {
        let second = saved
            .sector_two_best_ms
            .filter(|&ms| ms > 0)
            .ok_or("the save has no Sector 2 clear time")?;
        let (rectangles, solid) = observation
            .merged_tiles
            .ok_or("Sector 2 tile colliders were not presented")?;
        println!(
            "NIGHT_RELAY_R2 sectors=2 retired={} audio_output={:?} arrival_peak={:.6} \
             clear_ms={best},{second} save_round_trip=true camera_follow=true camera_shake=true \
             sprite_animation=true property_animation=true gate_tween=true additive_particles=true \
             bloom=true fade=true merged_tiles={rectangles}/{solid} steady_zero_frames={} \
             presented={}x{} hud_glyphs={}",
            observation.retired,
            observation.output,
            observation.peak,
            observation.quiet_frames,
            frame.extent.width,
            frame.extent.height,
            frame.hud_glyph_primitives,
        );
    } else if saved.sector_two_best_ms.is_some() {
        return Err("the Sector 1 probe unexpectedly saved a Sector 2 clear".into());
    } else {
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
    }
    if observation.output != AudioOutput::Device {
        return Err("the native mixer used audio_output=Mock".into());
    }
    Ok(())
}
