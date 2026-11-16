use std::collections::BTreeMap;

use nara::{
    asset::AssetRef,
    project_host::{DesktopPresentationObservation, ProjectDomainAssets},
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use serde_json::{Value, json};
use two_level_game::{
    GameSnapshot,
    audio::{self, AudioSnapshot, AudioWitness},
    levels::LevelKey,
    sound_bank::{SoundBankDomain, SoundCue, bank_product},
};

use super::{
    oracle::{self, CHANNELS, MAX_OUTPUT_FRAMES},
    runtime::PlayerExtension,
};

pub(super) struct AudioObservation {
    verify: bool,
    witness: Option<AudioWitness>,
    assets: Option<ProjectDomainAssets>,
    target: Option<BackendWindowHandles>,
    runtime: Option<u64>,
    instances: BTreeMap<u64, LevelKey>,
    output: Vec<f32>,
    captured_frames: u64,
    game: Option<GameSnapshot>,
    snapshot: Option<AudioSnapshot>,
    pcm: Option<Value>,
    native: Value,
    trajectory_completed: bool,
}

impl AudioObservation {
    pub(super) fn new(verify: bool) -> Self {
        Self {
            verify,
            witness: None,
            assets: None,
            target: None,
            runtime: None,
            instances: BTreeMap::new(),
            output: Vec::new(),
            captured_frames: 0,
            game: None,
            snapshot: None,
            pcm: None,
            native: Value::Null,
            trajectory_completed: false,
        }
    }

    fn drain(&mut self) -> Result<(), String> {
        let Some(witness) = &self.witness else {
            return Ok(());
        };
        for chunk in witness.drain_output() {
            if chunk.channels as usize != CHANNELS
                || chunk.first_channel != 0
                || chunk.samples.is_empty()
                || !chunk.samples.len().is_multiple_of(CHANNELS)
                || chunk.first_frame != self.captured_frames
            {
                return Err("cooked audio callback capture is unexpected or discontinuous".into());
            }
            self.captured_frames = self
                .captured_frames
                .checked_add((chunk.samples.len() / CHANNELS) as u64)
                .ok_or("cooked audio callback frame count exhausted")?;
            if !self.verify {
                continue;
            }
            if self.output.len() + chunk.samples.len() > MAX_OUTPUT_FRAMES as usize * CHANNELS {
                return Err("cooked audio callback capture exceeded its finite allowance".into());
            }
            self.output.extend(chunk.samples);
        }
        let snapshot = witness.snapshot();
        if snapshot.faults != 0 || snapshot.error.is_some() {
            return Err(format!(
                "original cooked audio device failed: {:?}; faults={:#x}",
                snapshot.error, snapshot.faults
            ));
        }
        if snapshot.runtime == 0
            || self
                .runtime
                .is_some_and(|runtime| runtime != snapshot.runtime)
        {
            return Err("cooked audio witness changed its original Runtime".into());
        }
        Ok(())
    }
}

impl PlayerExtension for AudioObservation {
    fn recipe(&self) -> nara::ProductRecipe {
        audio::cooked_recipe()
    }

    fn content_profile(&self) -> &'static str {
        "two-level-audio-desktop-shipping"
    }

    fn isolate_input(&self) -> bool {
        self.verify
    }

    fn poll(&mut self) -> Result<(), String> {
        self.drain()
    }

    fn observe(&mut self, facts: &DesktopPresentationObservation<'_>) -> Result<(), String> {
        if let Some(world) = facts.world() {
            self.witness = self.witness.take().or_else(|| audio::witness(world));
            self.target = self
                .target
                .take()
                .or_else(|| world.get_resource::<BackendWindowHandles>().cloned());
            if self.assets.is_none()
                && let Some(assets) = world.get_resource::<ProjectDomainAssets>()
                && let Some(banks) = assets.collection::<SoundBankDomain>()
                && banks.get(&AssetRef::Product(bank_product())).is_some()
            {
                if banks.iter().len() != 1 || banks.source_records().len() != 0 {
                    return Err("cooked SoundBank retained source records or extra banks".into());
                }
                self.assets = Some(assets.clone());
            }
            if self.verify
                && let Some(snapshot) = world.get_resource::<GameSnapshot>()
                && let (Some(instance), Some(level)) = (snapshot.current_instance, snapshot.level)
            {
                if self
                    .instances
                    .insert(instance.get(), level)
                    .is_some_and(|prior| prior != level)
                {
                    return Err("one cooked Scene instance changed its game-owned level".into());
                }
            }
            if facts.presented_frame().is_some() {
                self.game = world.get_resource::<GameSnapshot>().cloned();
            }
        }
        let runtime = facts.runtime_generation().get();
        if self
            .runtime
            .replace(runtime)
            .is_some_and(|prior| prior != runtime)
        {
            return Err("cooked audio observer changed its original Runtime".into());
        }
        self.drain()?;
        if self.verify && facts.presented_frame().is_some() {
            let adapter = facts
                .adapter_summary()
                .ok_or("original hardware adapter is unavailable")?;
            if std::env::var("WGPU_BACKEND").as_deref() != Ok(adapter.backend())
                || !matches!(adapter.device_type(), "discrete-gpu" | "integrated-gpu")
            {
                return Err(
                    "cooked audio verification requires the selected actual hardware backend"
                        .into(),
                );
            }
        }
        Ok(())
    }

    fn completed(&mut self) -> Result<bool, String> {
        self.drain()?;
        let game = self
            .game
            .as_ref()
            .ok_or("presented cooked game is unavailable")?;
        if game.travel_count != 4
            || game.restarts != 1
            || game.saves != 0
            || game.restores != 0
            || game.visible_pickups != 1
            || game.scene_members != 6
            || game.held != [false; 4]
            || game.player.to_array() != [-120.0, 0.0]
        {
            return Err("cooked audio Travel presented incorrect final gameplay facts".into());
        }
        let snapshot = self
            .witness
            .as_ref()
            .ok_or("original audio witness is unavailable")?
            .snapshot();
        self.trajectory_completed = snapshot.accepted == 7
            && snapshot.completed + snapshot.cancelled == 7
            && self.assets.is_some();
        Ok(self.trajectory_completed)
    }

    fn finish(&mut self) -> Result<(), String> {
        let drained = self.drain();
        self.snapshot = self.witness.as_ref().map(AudioWitness::snapshot);
        let target = self
            .target
            .as_ref()
            .ok_or("original native target is unavailable")?
            .snapshot(WindowId::PRIMARY)
            .map_err(|error| error.to_string())?;
        self.native = json!({
            "native_destroyed": target.native_destroyed && target.phase == WindowTargetPhase::NativeDestroyed,
            "surface_retired": !target.surface_active,
            "provider_released": !target.provider_present,
            "target_fault_absent": target.fault.is_none(),
        });
        drained?;
        if !target.native_destroyed
            || target.phase != WindowTargetPhase::NativeDestroyed
            || target.surface_active
            || target.provider_present
            || target.fault.is_some()
        {
            return Err(
                "original cooked audio window/surface/provider did not physically retire".into(),
            );
        }
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or("original audio witness was never published")?;
        if !snapshot.play_admitted
            || !snapshot.physical_complete
            || !snapshot.stream_drop_returned
            || !snapshot.data_callback_released
            || !snapshot.error_callback_released
            || snapshot.admission_open
            || snapshot.output_frames != self.captured_frames
        {
            return Err("original cooked audio stream/callback retirement is incomplete".into());
        }
        if self.verify {
            if !self.trajectory_completed {
                return Err(
                    "cooked audio trajectory did not complete before Runtime retirement".into(),
                );
            }
            let banks = self
                .assets
                .as_ref()
                .and_then(|assets| assets.collection::<SoundBankDomain>())
                .ok_or("original acquired SoundBank collection is unavailable")?;
            let bank = banks
                .get(&AssetRef::Product(bank_product()))
                .ok_or("original acquired SoundBank Product is unavailable")?;
            self.pcm = Some(oracle::verify(
                snapshot,
                &self.output,
                bank.samples(SoundCue::Pickup),
                bank.samples(SoundCue::Travel),
                &self.instances,
            )?);
        }
        Ok(())
    }

    fn evidence(&self) -> Option<Value> {
        Some(json!({
            "format": "two-level-cooked-audio/v1",
            "verification_requested": self.verify,
            "trajectory": if self.verify { "ordinary_travel_script" } else { "interactive" },
            "trajectory_completed": self.trajectory_completed,
            "bank_product": bank_product().to_string(),
            "source_free": true,
            "snapshot": self.snapshot,
            "callback_frames_drained": self.captured_frames,
            "pcm": self.pcm,
            "native": self.native,
            "deadline_scope": "cooperative original polls and native calls; callback samples do not establish human audibility",
            "observation_scope": "fixed callback capture is drained during all playback; only --verify retains PCM; no production performance claim",
            "verification_input": "initial isolation, then original focus and physical-key reconciliation before Resume",
        }))
    }
}
