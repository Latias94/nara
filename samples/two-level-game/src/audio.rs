//! The game's optional Windows CPAL one-shots and original managed device retirement.

mod bank;
mod clips;
mod device;
#[cfg(test)]
mod game_tests;
mod mixer;
#[cfg(test)]
mod native_tests;

use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, Mutex},
};

use nara::{
    ProductConfiguration, ProductRecipe,
    advanced_prelude::CurrentScene,
    app::{
        PluginCategory, PluginDeclaration, PluginId, PluginShutdownObligationId,
        RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
        RuntimeCloseParticipantId, RuntimeCloseProgress, RuntimeGeneration, StartupStage,
    },
    ecs::{World, bevy_ecs::error::BevyError},
    identity::SceneEntityId,
    prelude::{App, CoreStage, Plugin, PluginError, Resource},
};
use serde::Serialize;

use crate::{RunOptions, levels::LevelKey};
use bank::RetainedSoundBank;
use clips::{ClipKind, PreparedClips};
use device::{
    AudioController, AudioDevice, AudioWitness as DeviceWitness, CaptureMode, CueRecordKind,
    DeviceRecord, DeviceSettings, DeviceSnapshot,
};
use mixer::{Cue, EffectId, Scope};

pub const AUDIO_PLUGIN_ID: PluginId = PluginId::new("two-level-game.audio-cpal");
const CLOSE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("two-level-game.audio-cpal.close");
const CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("two-level-game.audio-cpal.close");
const MAX_CAUSES: usize = 4096;
const MAX_CUE_RECORDS: usize = 3 * MAX_CAUSES;
const MAX_SCOPE_RECORDS: usize = 4096;

/// Selects bounded observation of this concrete device owner's output.
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioOptions {
    pub capture_output: bool,
}

impl AudioOptions {
    fn capture_mode(self) -> CaptureMode {
        if self.capture_output {
            CaptureMode::Strict
        } else {
            CaptureMode::Disabled
        }
    }
}

impl ProductConfiguration for AudioOptions {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        self.capture_mode().write_canonical(output);
    }
}

impl ProductConfiguration for CaptureMode {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"two-level-cpal-f32-stereo-48000/v1");
        output.push(*self as u8);
    }
}

/// An explicit audible source-game composition; ordinary and cooked recipes stay independent.
pub fn recipe(options: RunOptions, audio: AudioOptions) -> ProductRecipe {
    with_audio(crate::configured_recipe(options), audio.capture_mode())
}

/// Starts the original device with gameplay publication disabled and empty effect history.
///
/// Each rebuilt Runtime needs explicit enablement after adoption and predecessor retirement.
/// Suppressed pickup/travel effects are discarded and never replayed by enablement or resume.
/// Production development callers should leave `AudioOptions::capture_output` disabled.
pub fn development_recipe(options: RunOptions, audio: AudioOptions) -> ProductRecipe {
    crate::configured_recipe(options)
        .add_configured_plugin(DevelopmentAudioOptions(audio), |options| AudioPlugin {
            capture: options.0.capture_mode(),
            policy: AudioPolicy::ResetOnRebuild,
            native_gate: None,
            #[cfg(test)]
            test_clips: None,
        })
        .expect("the game's fixed development audio contribution is valid")
}

struct DevelopmentAudioOptions(AudioOptions);

impl ProductConfiguration for DevelopmentAudioOptions {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        self.0.write_canonical(output);
        output.extend_from_slice(b"two-level-game.audio.reset-on-rebuild.discard-until-enabled/v1");
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Resource)]
enum AudioPolicy {
    Ordinary,
    ResetOnRebuild,
}

/// The explicitly observed audible Player placement selected by the cooker.
#[cfg(feature = "desktop")]
pub fn cooked_recipe() -> ProductRecipe {
    with_audio(crate::cooked_desktop_recipe(), CaptureMode::BestEffort)
}

fn with_audio(recipe: ProductRecipe, capture: CaptureMode) -> ProductRecipe {
    recipe
        .add_configured_plugin(capture, |capture| AudioPlugin {
            capture: *capture,
            policy: AudioPolicy::Ordinary,
            native_gate: None,
            #[cfg(test)]
            test_clips: None,
        })
        .expect("the game's fixed audio contribution is valid")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioClip {
    Pickup,
    Travel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioCause {
    Pickup { level: LevelKey, entity: String },
    Travel { level: LevelKey },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioPhase {
    Accepted,
    Duplicate,
    Started,
    Completed,
    Cancelled { started: bool },
    Refused { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioRecord {
    Cue {
        runtime: u64,
        instance: u64,
        sequence: u64,
        clip: AudioClip,
        cause: AudioCause,
        frame: u64,
        phase: AudioPhase,
    },
    ScopeApplied {
        runtime: u64,
        instance: u64,
        frame: u64,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct AudioSnapshot {
    pub runtime: u64,
    pub reset_on_rebuild: bool,
    pub output_enabled: bool,
    pub suppressed_effects: u64,
    pub desired_instance: u64,
    pub applied_instance: u64,
    pub play_admitted: bool,
    pub callbacks: u64,
    pub output_frames: u64,
    pub accepted: u64,
    pub started: u64,
    pub completed: u64,
    pub cancelled: u64,
    pub faults: u64,
    pub admission_open: bool,
    pub stream_drop_returned: bool,
    pub data_callback_released: bool,
    pub error_callback_released: bool,
    pub physical_complete: bool,
    pub error: Option<String>,
    pub records: Vec<AudioRecord>,
}

/// A copy of this game's own callback output, rather than system or microphone capture.
#[derive(Clone, Debug)]
pub struct AudioOutput {
    pub first_frame: u64,
    pub first_channel: u16,
    pub channels: u16,
    pub samples: Vec<f32>,
}

struct GameAudioState {
    control: AudioController,
    device: DeviceWitness,
    policy: AudioPolicy,
    output_enabled: bool,
    suppressed_effects: u64,
    next_sequence: u64,
    causes: BTreeMap<EffectId, AudioCause>,
    journal: AudioJournal,
    error: Option<String>,
}

#[derive(Default)]
struct AudioJournal {
    records: Vec<AudioRecord>,
    cues: usize,
    scopes: usize,
}

impl AudioJournal {
    fn push(&mut self, record: AudioRecord) -> Result<(), &'static str> {
        let (count, limit, error) = match &record {
            AudioRecord::Cue { .. } => (
                &mut self.cues,
                MAX_CUE_RECORDS,
                "audio cue record capacity exhausted",
            ),
            AudioRecord::ScopeApplied { .. } => (
                &mut self.scopes,
                MAX_SCOPE_RECORDS,
                "audio scope record capacity exhausted",
            ),
        };
        if *count == limit {
            return Err(error);
        }
        *count += 1;
        self.records.push(record);
        Ok(())
    }
}

/// Observation and explicit development publication control without retaining the native device.
///
/// This witness survives World destruction. A retired or faulted owner cannot enable output.
#[derive(Clone)]
pub struct AudioWitness(Arc<Mutex<GameAudioState>>);

impl AudioWitness {
    pub fn snapshot(&self) -> AudioSnapshot {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.drain();
        let native = state.device.snapshot();
        AudioSnapshot {
            runtime: native.runtime,
            reset_on_rebuild: state.policy == AudioPolicy::ResetOnRebuild,
            output_enabled: state.output_enabled && state.check_live(native).is_ok(),
            suppressed_effects: state.suppressed_effects,
            desired_instance: native.desired_instance,
            applied_instance: native.applied_instance,
            play_admitted: native.play_admitted,
            callbacks: native.callback_count,
            output_frames: native.output_frame,
            accepted: native.counters.accepted,
            started: native.counters.started,
            completed: native.counters.completed,
            cancelled: native.counters.cancelled,
            faults: native.faults,
            admission_open: native.admission_open,
            stream_drop_returned: native.stream_drop_returned,
            data_callback_released: native.data_callback_released,
            error_callback_released: native.error_callback_released,
            physical_complete: native.physical_complete,
            error: state.error.clone(),
            records: state.journal.records.clone(),
        }
    }

    /// Enables only future cues on this live development Runtime's original audio owner.
    ///
    /// The caller must first adopt this Runtime and physically retire its predecessor. This
    /// gate checks the reset policy and original device health/admission, not Host adoption.
    /// Repeated enablement is harmless while that owner remains live. Ordinary recipes refuse.
    pub fn enable_development_output(&self) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "game audio state is poisoned")?;
        state.drain();
        if state.policy != AudioPolicy::ResetOnRebuild {
            return Err("audio output enablement requires the development reset policy".into());
        }
        state.check_live(state.device.snapshot())?;
        state.output_enabled = true;
        Ok(())
    }

    pub fn drain_output(&self) -> Vec<AudioOutput> {
        let state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .device
            .drain_capture()
            .into_iter()
            .map(|chunk| AudioOutput {
                first_frame: chunk.first_frame,
                first_channel: chunk.first_channel,
                channels: chunk.channels,
                samples: chunk.samples[..chunk.len].to_vec(),
            })
            .collect()
    }

    /// Cumulative stereo frames dropped by the bounded callback capture.
    pub fn dropped_output_frames(&self) -> u64 {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .device
            .dropped_output_frames()
    }
}

impl GameAudioState {
    fn new(control: AudioController, device: DeviceWitness, policy: AudioPolicy) -> Self {
        Self {
            control,
            device,
            policy,
            output_enabled: policy == AudioPolicy::Ordinary,
            suppressed_effects: 0,
            next_sequence: 1,
            causes: BTreeMap::new(),
            journal: AudioJournal::default(),
            error: None,
        }
    }

    fn check_live(&self, native: DeviceSnapshot) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if native.faults != 0 {
            return Err(format!("native audio fault mask: {:#x}", native.faults));
        }
        if native.runtime == 0
            || !native.stream_created
            || !native.play_admitted
            || !native.admission_open
            || native.quiescing
            || native.stream_drop_returned
            || native.data_callback_released
            || native.error_callback_released
            || native.physical_complete
        {
            return Err("the original audio owner is not live with open playback admission".into());
        }
        Ok(())
    }

    fn publish(&mut self, scope: Scope, clip: ClipKind, cause: AudioCause) {
        self.drain();
        if !self.output_enabled {
            self.suppressed_effects = self.suppressed_effects.saturating_add(1);
            if let Err(error) = self.control.set_scope(scope) {
                self.error(format!("game audio scope refused: {error:?}"));
            }
            return;
        }
        if self.error.is_some() {
            return;
        }
        if self.causes.len() >= MAX_CAUSES {
            self.error("game audio effect capacity exhausted".into());
            return;
        }
        let Some(next) = self.next_sequence.checked_add(1) else {
            self.error("game audio effect identity exhausted".into());
            return;
        };
        let cue = Cue {
            id: EffectId {
                scope,
                sequence: self.next_sequence,
            },
            clip,
        };
        self.next_sequence = next;
        self.causes.insert(cue.id, cause);
        if let Err(error) = self.control.set_scope(scope) {
            self.error(format!("game audio scope refused: {error:?}"));
        } else if let Err(error) = self.control.submit(cue) {
            self.error(format!("committed game effect was refused: {error:?}"));
        }
    }

    fn error(&mut self, message: String) {
        if self.error.is_none() {
            eprintln!("two_level_audio: {message}");
            self.error = Some(message);
        }
    }

    fn drain(&mut self) {
        for record in self.device.drain_records() {
            let record = match record {
                DeviceRecord::ScopeApplied { scope, frame } => AudioRecord::ScopeApplied {
                    runtime: scope.runtime,
                    instance: scope.instance,
                    frame,
                },
                DeviceRecord::Cue { cue, frame, kind } => {
                    let Some(cause) = self.causes.get(&cue.id).cloned() else {
                        self.error("device reported an unknown game effect".into());
                        continue;
                    };
                    let phase = match kind {
                        CueRecordKind::Accepted => AudioPhase::Accepted,
                        CueRecordKind::Duplicate => AudioPhase::Duplicate,
                        CueRecordKind::Started => AudioPhase::Started,
                        CueRecordKind::Completed => AudioPhase::Completed,
                        CueRecordKind::Cancelled { started } => AudioPhase::Cancelled { started },
                        CueRecordKind::Refused(reason) => AudioPhase::Refused {
                            reason: format!("{reason:?}"),
                        },
                    };
                    AudioRecord::Cue {
                        runtime: cue.id.scope.runtime,
                        instance: cue.id.scope.instance,
                        sequence: cue.id.sequence,
                        clip: match cue.clip {
                            ClipKind::Pickup => AudioClip::Pickup,
                            ClipKind::Travel => AudioClip::Travel,
                        },
                        cause,
                        frame,
                        phase,
                    }
                }
            };
            if let Err(error) = self.journal.push(record) {
                self.error(error.into());
            }
        }
    }
}

#[derive(Resource)]
struct GameAudio {
    witness: AudioWitness,
    bank: Option<RetainedSoundBank>,
}

/// Observes only this Runtime's original optional audio owner.
pub fn witness(world: &World) -> Option<AudioWitness> {
    world
        .get_resource::<GameAudio>()
        .map(|audio| audio.witness.clone())
}

pub(crate) fn bank_guard(world: &World) -> Option<RetainedSoundBank> {
    world
        .get_resource::<GameAudio>()
        .and_then(|audio| audio.bank.clone())
}

pub(crate) fn development_reset_installed(world: &World) -> bool {
    world.get_resource::<AudioPolicy>() == Some(&AudioPolicy::ResetOnRebuild)
}

pub(crate) fn pickup_collected(world: &mut World, level: LevelKey, entity: SceneEntityId) {
    publish(
        world,
        ClipKind::Pickup,
        AudioCause::Pickup {
            level,
            entity: entity.as_str().to_owned(),
        },
    );
}

pub(crate) fn travel_entered(world: &mut World, level: LevelKey) {
    publish(world, ClipKind::Travel, AudioCause::Travel { level });
}

fn scope(world: &World) -> Option<Scope> {
    Some(Scope {
        runtime: world.get_resource::<RuntimeGeneration>()?.get(),
        instance: CurrentScene::get(world)?.receipt().instance_id().get(),
    })
}

pub(crate) fn scene_replaced(world: &mut World) {
    let (Some(audio), Some(scope)) = (witness(world), scope(world)) else {
        return;
    };
    let mut state = audio
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.drain();
    if let Err(error) = state.control.set_scope(scope) {
        state.error(format!("scene audio retirement refused: {error:?}"));
    }
}

fn publish(world: &mut World, clip: ClipKind, cause: AudioCause) {
    let (Some(audio), Some(scope)) = (witness(world), scope(world)) else {
        return;
    };
    audio
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .publish(scope, clip, cause);
}

struct AudioPlugin {
    capture: CaptureMode,
    policy: AudioPolicy,
    native_gate: Option<device::NativeGate>,
    #[cfg(test)]
    test_clips: Option<PreparedClips>,
}

#[derive(Default)]
struct DeviceSlot {
    device: Option<AudioDevice>,
    failed_retirement: Option<FailedRetirement>,
}

struct FailedRetirement {
    last: DeviceSnapshot,
    witness: Option<DeviceWitness>,
}

struct NativeOwner {
    slot: Rc<RefCell<DeviceSlot>>,
    settings: DeviceSettings,
    #[cfg(test)]
    test_clips: Option<PreparedClips>,
}

impl Plugin for AudioPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(AUDIO_PLUGIN_ID, PluginCategory::Runtime)
                .shutdown_obligations(&[CLOSE_OBLIGATION]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let slot = Rc::new(RefCell::new(DeviceSlot::default()));
        app.register_plugin_runtime_close_participant(
            CLOSE_OBLIGATION,
            CLOSE_PARTICIPANT,
            AudioClose(slot.clone()),
        )?;
        app.insert_resource(self.policy)?;
        app.world_mut()?.insert_non_send(NativeOwner {
            slot,
            settings: DeviceSettings {
                capture: self.capture,
                native_gate: self.native_gate.clone(),
            },
            #[cfg(test)]
            test_clips: self.test_clips.clone(),
        });
        app.add_systems(StartupStage::Tooling, start_audio)?;
        app.add_systems(CoreStage::Last, maintain_audio)?;
        Ok(())
    }
}

fn start_audio(world: &mut World) -> Result<(), BevyError> {
    let runtime = world.resource::<RuntimeGeneration>().get();
    let policy = *world.resource::<AudioPolicy>();
    let owner = world.non_send::<NativeOwner>();
    let slot = owner.slot.clone();
    let settings = owner.settings.clone();
    #[cfg(test)]
    let injected = owner.test_clips.clone();
    #[cfg(not(test))]
    let injected: Option<PreparedClips> = None;
    let (clips, bank) = match injected {
        Some(clips) => (clips, None),
        None => {
            let reference = crate::gameplay::sound_bank_reference(world)
                .map_err(|error| BevyError::error(std::io::Error::other(error)))?;
            let assets = world
                .get_resource::<nara::project_host::ProjectDomainAssets>()
                .ok_or_else(|| {
                    BevyError::error(std::io::Error::other(
                        "audio startup lacks acquired content",
                    ))
                })?;
            let bank = RetainedSoundBank::acquire(assets, &reference)
                .map_err(|error| BevyError::error(std::io::Error::other(error)))?;
            (PreparedClips::from_bank(&bank), Some(bank))
        }
    };
    match AudioDevice::open(runtime, clips, settings) {
        Ok(device) => {
            let state = GameAudioState::new(device.control(), device.witness(), policy);
            slot.borrow_mut().device = Some(device);
            world.insert_resource(GameAudio {
                witness: AudioWitness(Arc::new(Mutex::new(state))),
                bank,
            });
            Ok(())
        }
        Err(mut error) => {
            let message = error.to_string();
            let mut slot = slot.borrow_mut();
            slot.device = error.retained_device.take();
            slot.failed_retirement = Some(FailedRetirement {
                last: error.retirement,
                witness: error.witness.take(),
            });
            Err(BevyError::error(std::io::Error::other(message)))
        }
    }
}

fn maintain_audio(world: &mut World) {
    if let Some(audio) = witness(world) {
        let mut state = audio
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.drain();
        let faults = state.device.snapshot().faults;
        if faults != 0 {
            state.error(format!("native audio fault mask: {faults:#x}"));
        }
    }
}

struct AudioClose(Rc<RefCell<DeviceSlot>>);

impl AudioClose {
    fn close(&mut self) -> RuntimeCloseProgress {
        let mut slot = self.0.borrow_mut();
        if let Some(device) = &mut slot.device {
            device.quiesce();
            let report = device.close();
            if !report.complete {
                return RuntimeCloseProgress::Pending;
            }
            slot.device.take();
            return if report.faults == 0 {
                RuntimeCloseProgress::Complete
            } else {
                RuntimeCloseProgress::CompleteWithError {
                    code: "two-level-game.audio-cpal.close",
                }
            };
        }
        let failed = slot.failed_retirement.as_ref().map(|failed| {
            failed
                .witness
                .as_ref()
                .map_or(failed.last, DeviceWitness::snapshot)
        });
        match failed {
            Some(failed) if !failed.physical_complete => RuntimeCloseProgress::Pending,
            Some(failed) if failed.faults != 0 => RuntimeCloseProgress::CompleteWithError {
                code: "two-level-game.audio-cpal.startup",
            },
            _ => RuntimeCloseProgress::Complete,
        }
    }
}

impl RuntimeCloseParticipant for AudioClose {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.close())
    }
    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.close())
    }
}
