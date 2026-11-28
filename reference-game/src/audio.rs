use std::{f32::consts::TAU, time::Duration};

use nara::{asset::add_asset, ecs::bevy_ecs::error::Result, prelude::*};

pub(crate) const ARRIVAL_BUS: &str = "arrival";

#[derive(Resource)]
pub(crate) struct Sounds {
    pub(crate) pickup: Handle<AudioClip>,
    pub(crate) impact: Handle<AudioClip>,
    pub(crate) escaped: Handle<AudioClip>,
    pub(crate) expired: Handle<AudioClip>,
}

impl Sounds {
    pub(crate) fn register(world: &mut World) -> Result<Self> {
        Ok(Self {
            pickup: add_clip(world, &[(523.25, 65), (783.99, 105)])?,
            impact: add_clip(world, &[(185.0, 70), (116.54, 105)])?,
            escaped: add_clip(world, &[(523.25, 90), (659.25, 90), (1046.50, 230)])?,
            expired: add_clip(world, &[(246.94, 140), (185.0, 230)])?,
        })
    }
}

fn add_clip(world: &mut World, notes: &[(f32, u16)]) -> Result<Handle<AudioClip>> {
    let clip = AudioClip::from_bytes(waveform(notes))?;
    Ok(add_asset(world, clip)?)
}

fn waveform(notes: &[(f32, u16)]) -> Vec<u8> {
    const SAMPLE_RATE: u32 = 22_050;
    let sample_count = |milliseconds: u16| SAMPLE_RATE * u32::from(milliseconds) / 1_000;
    let data_len = 2 * notes.iter().map(|&(_, ms)| sample_count(ms)).sum::<u32>();
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt \x10\x00\x00\x00\x01\x00\x01\x00");
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(2 * SAMPLE_RATE).to_le_bytes());
    wav.extend_from_slice(b"\x02\x00\x10\x00data");
    wav.extend_from_slice(&data_len.to_le_bytes());

    for &(frequency, milliseconds) in notes {
        let duration = Duration::from_millis(u64::from(milliseconds)).as_secs_f32();
        for sample in 0..sample_count(milliseconds) {
            let age = sample as f32 / SAMPLE_RATE as f32;
            let attack = (age * 180.0).min(1.0);
            let release = ((duration - age) * 32.0).clamp(0.0, 1.0);
            let phase = TAU * frequency * age;
            let signal = (phase.sin() + 0.18 * (2.0 * phase).sin()) * 0.25;
            wav.extend_from_slice(
                &((signal * attack * release * i16::MAX as f32) as i16).to_le_bytes(),
            );
        }
    }
    wav
}
