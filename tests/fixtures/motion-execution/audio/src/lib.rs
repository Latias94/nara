#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    error::Error,
    f64::consts::TAU,
    fmt::{self, Display, Formatter},
    sync::{Arc, Mutex},
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
};

pub const MAX_VOICES: usize = 128;
pub const MAX_LEDGER_ENTRIES: usize = 4096;
pub const AUDIO_PLUGIN_ID: PluginId = PluginId::new("fixture.motion.audio");
const AUDIO_SHUTDOWN: PluginShutdownObligationId =
    PluginShutdownObligationId::new("fixture.motion.audio.shutdown");
const AUDIO_CLOSE: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("fixture.motion.audio.close");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticEffectId {
    pub input_event: u64,
    pub occurrence: u64,
    pub cue: u32,
    pub sequence: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cue {
    pub id: SemanticEffectId,
    pub at_sample: u64,
    pub sample_count: u32,
    pub frequency_hz: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioError {
    Closed,
    InvalidSampleRate,
    InvalidFrequency,
    InvalidDuration,
    SampleOverflow,
    LateCue { id: SemanticEffectId },
    Conflict { id: SemanticEffectId },
    VoiceCapacity,
    LedgerCapacity,
    AllocationFailed,
}

impl Display for AudioError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => formatter.write_str("PCM audio admission is closed"),
            Self::InvalidSampleRate => formatter.write_str("sample rate must be positive"),
            Self::InvalidFrequency => {
                formatter.write_str("frequency must be positive and below Nyquist")
            }
            Self::InvalidDuration => formatter.write_str("sample count must be positive"),
            Self::SampleOverflow => formatter.write_str("sample position overflow"),
            Self::LateCue { id } => write!(formatter, "cue {id:?} starts before the sample cursor"),
            Self::Conflict { id } => write!(formatter, "cue {id:?} has conflicting payloads"),
            Self::VoiceCapacity => formatter.write_str("PCM voice capacity exhausted"),
            Self::LedgerCapacity => formatter.write_str("PCM effect ledger capacity exhausted"),
            Self::AllocationFailed => formatter.write_str("PCM output allocation failed"),
        }
    }
}

impl Error for AudioError {}

/// Offline mono PCM owner. Its effect ledger is independent of model checkpoints.
#[derive(Debug)]
pub struct PcmAudio {
    sample_rate: u32,
    cursor: u64,
    voices: Vec<Cue>,
    ledger: BTreeMap<SemanticEffectId, Cue>,
    closed: bool,
}

impl PcmAudio {
    pub fn new(sample_rate: u32) -> Result<Self, AudioError> {
        if sample_rate == 0 {
            return Err(AudioError::InvalidSampleRate);
        }
        Ok(Self {
            sample_rate,
            cursor: 0,
            voices: Vec::new(),
            ledger: BTreeMap::new(),
            closed: false,
        })
    }

    pub fn cursor(&self) -> u64 {
        self.cursor
    }

    /// Atomically admits new cues, returning their count; identical repeats are suppressed.
    pub fn publish(&mut self, cues: &[Cue]) -> Result<usize, AudioError> {
        if self.closed {
            return Err(AudioError::Closed);
        }
        let mut pending = BTreeMap::new();
        for cue in cues {
            if let Some(previous) = self.ledger.get(&cue.id).or_else(|| pending.get(&cue.id)) {
                if previous != cue {
                    return Err(AudioError::Conflict { id: cue.id });
                }
                continue;
            }
            if cue.sample_count == 0 {
                return Err(AudioError::InvalidDuration);
            }
            if cue.frequency_hz == 0
                || u64::from(cue.frequency_hz) * 2 >= u64::from(self.sample_rate)
            {
                return Err(AudioError::InvalidFrequency);
            }
            cue.at_sample
                .checked_add(u64::from(cue.sample_count))
                .ok_or(AudioError::SampleOverflow)?;
            if cue.at_sample < self.cursor {
                return Err(AudioError::LateCue { id: cue.id });
            }
            if self.voices.len() + pending.len() >= MAX_VOICES {
                return Err(AudioError::VoiceCapacity);
            }
            if self.ledger.len() + pending.len() >= MAX_LEDGER_ENTRIES {
                return Err(AudioError::LedgerCapacity);
            }
            pending.insert(cue.id, *cue);
        }
        let admitted = pending.len();
        self.voices.extend(pending.values().copied());
        self.voices.sort_unstable_by_key(|cue| cue.id);
        self.ledger.extend(pending);
        Ok(admitted)
    }

    /// Renders [cursor, cursor + frame_count), mixing 0.2-amplitude sine voices with clipping.
    /// Phase depends on absolute sample position relative to cue onset, never render chunk size.
    pub fn render(&mut self, frame_count: usize) -> Result<Vec<f32>, AudioError> {
        if self.closed {
            return Err(AudioError::Closed);
        }
        let count = u64::try_from(frame_count).map_err(|_| AudioError::SampleOverflow)?;
        let end = self
            .cursor
            .checked_add(count)
            .ok_or(AudioError::SampleOverflow)?;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(frame_count)
            .map_err(|_| AudioError::AllocationFailed)?;
        for position in self.cursor..end {
            let mut mixed = 0.0_f32;
            for cue in &self.voices {
                if position >= cue.at_sample
                    && position - cue.at_sample < u64::from(cue.sample_count)
                {
                    let phase = ((position - cue.at_sample) * u64::from(cue.frequency_hz))
                        % u64::from(self.sample_rate);
                    let angle = TAU * phase as f64 / f64::from(self.sample_rate);
                    mixed += 0.2 * angle.sin() as f32;
                }
            }
            samples.push(mixed.clamp(-1.0, 1.0));
        }
        self.cursor = end;
        self.voices
            .retain(|cue| cue.at_sample + u64::from(cue.sample_count) > end);
        Ok(samples)
    }

    pub fn shutdown(&mut self) {
        self.closed = true;
        self.voices.clear();
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

pub struct AudioPlugin {
    pub owner: Arc<Mutex<PcmAudio>>,
}

impl Plugin for AudioPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(AUDIO_PLUGIN_ID, PluginCategory::Runtime)
                .shutdown_obligations(&[AUDIO_SHUTDOWN]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            AUDIO_SHUTDOWN,
            AUDIO_CLOSE,
            AudioClose(Arc::clone(&self.owner)),
        )?;
        Ok(())
    }
}

struct AudioClose(Arc<Mutex<PcmAudio>>);

impl AudioClose {
    fn close(&mut self) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0
            .lock()
            .map_err(|_| RuntimeCloseParticipantError::terminal("fixture.motion.audio.poisoned"))?
            .shutdown();
        Ok(RuntimeCloseProgress::Complete)
    }
}

impl RuntimeCloseParticipant for AudioClose {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.close()
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.close()
    }
}
