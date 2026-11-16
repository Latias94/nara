//! Fixed-capacity CPU mixing for committed game one-shots on an independent device clock.

use super::clips::{ClipKind, PreparedClips};

pub(super) const OUTPUT_CHANNELS: usize = 2;
pub(super) const VOICE_CAPACITY: usize = 16;
pub(super) const EFFECT_CAPACITY: usize = 4096;
pub(super) const EVENT_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Scope {
    pub runtime: u64,
    pub instance: u64,
}

impl Scope {
    fn validate(self) -> Result<(), Refusal> {
        if self.runtime == 0 || self.instance == 0 {
            return Err(Refusal::InvalidScope);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct EffectId {
    pub scope: Scope,
    pub sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cue {
    pub id: EffectId,
    pub clip: ClipKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Publication {
    Accepted,
    Duplicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    InvalidScope,
    ScopeNotSet,
    ForeignRuntime,
    StaleScope,
    ForeignScope,
    InvalidSequence,
    StaleSequence,
    SequenceOverflow,
    ConflictingCue { existing: ClipKind },
    VoiceCapacity,
    EffectCapacity,
    EventCapacity,
    FrameOverflow,
    NonStereoOutput,
}

/// All counters are bounded by the non-evicting effect ledger, including across scope changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Counters {
    pub accepted: u64,
    pub started: u64,
    pub completed: u64,
    pub cancelled: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EventKind {
    Started,
    Completed,
    Cancelled { started: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Event {
    pub cue: Cue,
    /// Started: first sample; Completed: exclusive end; Cancelled: next output frame at the cut.
    /// These are callback output positions, not physical playback or receipt observations.
    pub frame: u64,
    pub kind: EventKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RenderReport {
    pub first_frame: u64,
    pub end_frame: u64,
    pub before: Counters,
    pub after: Counters,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Voice {
    cue: Cue,
    cursor: usize,
    started: bool,
}

/// One mixer belongs to one Runtime. Instances advance monotonically; reselecting the current
/// instance is a no-op. A fresh Runtime needs a fresh mixer, never a reconstructed audio state.
///
/// Every acceptance reserves two event slots. Start consumes one; completion consumes the other.
/// Cancellation before start consumes one and releases the unused reservation. Thus accepted
/// voices can always report a terminal fact, even when the caller stops draining events. New
/// publication then refuses with EventCapacity. Drain through pop_event; no event is overwritten.
///
/// The exact effect ledger lasts for the mixer lifetime and never evicts. Exhaustion refuses new
/// effects rather than forgetting an identity. Construction is the only allocation in this type;
/// its methods do not clone/drop clip Arcs, allocate, lock, or touch game time or a native device.
pub(super) struct Mixer {
    clips: PreparedClips,
    scope: Option<Scope>,
    last_sequence: u64,
    effects: Box<[Option<Cue>]>,
    effect_count: usize,
    voices: [Option<Voice>; VOICE_CAPACITY],
    events: [Option<Event>; EVENT_CAPACITY],
    event_head: usize,
    event_count: usize,
    reserved_events: usize,
    output_frame: u64,
    counters: Counters,
}

impl Mixer {
    pub(super) fn new(clips: PreparedClips) -> Self {
        Self {
            clips,
            scope: None,
            last_sequence: 0,
            effects: vec![None; EFFECT_CAPACITY].into_boxed_slice(),
            effect_count: 0,
            voices: [None; VOICE_CAPACITY],
            events: [None; EVENT_CAPACITY],
            event_head: 0,
            event_count: 0,
            reserved_events: 0,
            output_frame: 0,
            counters: Counters::default(),
        }
    }

    /// Apply a control before rendering later output. Refusal leaves the entire mixer unchanged.
    /// Sequence monotonicity is per instance; callers must use checked identity increments.
    pub(super) fn set_scope(&mut self, scope: Scope) -> Result<(), Refusal> {
        scope.validate()?;
        if let Some(current) = self.scope {
            if current.runtime != scope.runtime {
                return Err(Refusal::ForeignRuntime);
            }
            if scope.instance < current.instance {
                return Err(Refusal::StaleScope);
            }
            if scope == current {
                return Ok(());
            }
        }
        self.cancel_all();
        self.scope = Some(scope);
        self.last_sequence = 0;
        Ok(())
    }

    /// Cancels accepted voices at the next output position while retaining identity history.
    pub(super) fn cancel_all(&mut self) {
        for index in 0..VOICE_CAPACITY {
            if let Some(voice) = self.voices[index].take() {
                if !voice.started {
                    self.reserved_events -= 1;
                }
                self.record_event(Event {
                    cue: voice.cue,
                    frame: self.output_frame,
                    kind: EventKind::Cancelled {
                        started: voice.started,
                    },
                });
                self.counters.cancelled += 1;
            }
        }
    }

    /// Accept the complete cue or refuse without changing voices, identities, counters or events.
    /// Exact duplicates stay Duplicate after completion and even when capacities are exhausted.
    pub(super) fn publish(&mut self, cue: Cue) -> Result<Publication, Refusal> {
        cue.id.scope.validate()?;
        if cue.id.sequence == 0 {
            return Err(Refusal::InvalidSequence);
        }
        let current = self.scope.ok_or(Refusal::ScopeNotSet)?;
        if cue.id.scope.runtime != current.runtime {
            return Err(Refusal::ForeignRuntime);
        }
        if cue.id.scope.instance < current.instance {
            return Err(Refusal::StaleScope);
        }
        if cue.id.scope != current {
            return Err(Refusal::ForeignScope);
        }
        if let Some(existing) = self.effects[..self.effect_count]
            .iter()
            .flatten()
            .find(|existing| existing.id == cue.id)
        {
            return if existing.clip == cue.clip {
                Ok(Publication::Duplicate)
            } else {
                Err(Refusal::ConflictingCue {
                    existing: existing.clip,
                })
            };
        }
        if self.last_sequence == u64::MAX {
            return Err(Refusal::SequenceOverflow);
        }
        if cue.id.sequence <= self.last_sequence {
            return Err(Refusal::StaleSequence);
        }
        if self.effect_count == EFFECT_CAPACITY {
            return Err(Refusal::EffectCapacity);
        }
        let voice_index = self
            .voices
            .iter()
            .position(Option::is_none)
            .ok_or(Refusal::VoiceCapacity)?;
        if self.event_count + self.reserved_events + 2 > EVENT_CAPACITY {
            return Err(Refusal::EventCapacity);
        }
        let frames = self.clips.samples(cue.clip).len() as u64;
        self.output_frame
            .checked_add(frames)
            .ok_or(Refusal::FrameOverflow)?;

        self.effects[self.effect_count] = Some(cue);
        self.effect_count += 1;
        self.last_sequence = cue.id.sequence;
        self.voices[voice_index] = Some(Voice {
            cue,
            cursor: 0,
            started: false,
        });
        self.reserved_events += 2;
        self.counters.accepted += 1;
        Ok(Publication::Accepted)
    }

    /// Fill interleaved stereo at exactly 48 kHz. Mono voices sum in slot order, then hard-clip to
    /// [-1, 1]. There is no resampling, interpolation, game-pause input, or callback allocation.
    /// Invalid buffers/clock overflow return silence and leave all mixer state unchanged. The
    /// caller must surface that refusal rather than interpreting silence as successful progress.
    pub(super) fn render(&mut self, output: &mut [f32]) -> Result<RenderReport, Refusal> {
        output.fill(0.0);
        if !output.len().is_multiple_of(OUTPUT_CHANNELS) {
            return Err(Refusal::NonStereoOutput);
        }
        let frames =
            u64::try_from(output.len() / OUTPUT_CHANNELS).map_err(|_| Refusal::FrameOverflow)?;
        let end_frame = self
            .output_frame
            .checked_add(frames)
            .ok_or(Refusal::FrameOverflow)?;
        let before = self.counters;
        let first_frame = self.output_frame;
        if frames > 0 {
            for index in 0..VOICE_CAPACITY {
                if let Some(mut voice) = self.voices[index]
                    && !voice.started
                {
                    voice.started = true;
                    self.voices[index] = Some(voice);
                    self.record_event(Event {
                        cue: voice.cue,
                        frame: first_frame,
                        kind: EventKind::Started,
                    });
                    self.counters.started += 1;
                }
            }
        }
        for stereo in output.chunks_exact_mut(OUTPUT_CHANNELS) {
            let mut mixed = 0.0_f32;
            self.output_frame += 1;
            for index in 0..VOICE_CAPACITY {
                if let Some(mut voice) = self.voices[index] {
                    let samples = self.clips.samples(voice.cue.clip);
                    mixed += samples[voice.cursor];
                    voice.cursor += 1;
                    if voice.cursor == samples.len() {
                        self.voices[index] = None;
                        self.record_event(Event {
                            cue: voice.cue,
                            frame: self.output_frame,
                            kind: EventKind::Completed,
                        });
                        self.counters.completed += 1;
                    } else {
                        self.voices[index] = Some(voice);
                    }
                }
            }
            stereo.fill(mixed.clamp(-1.0, 1.0));
        }
        Ok(RenderReport {
            first_frame,
            end_frame,
            before,
            after: self.counters,
        })
    }

    pub(super) fn output_frame(&self) -> u64 {
        self.output_frame
    }

    pub(super) fn counters(&self) -> Counters {
        self.counters
    }

    pub(super) fn pop_event(&mut self) -> Option<Event> {
        if self.event_count == 0 {
            return None;
        }
        let event = self.events[self.event_head].take();
        self.event_head = (self.event_head + 1) % EVENT_CAPACITY;
        self.event_count -= 1;
        event
    }

    fn record_event(&mut self, event: Event) {
        let tail = (self.event_head + self.event_count) % EVENT_CAPACITY;
        self.events[tail] = Some(event);
        self.event_count += 1;
        self.reserved_events -= 1;
    }
}

#[cfg(test)]
#[path = "mixer_tests.rs"]
mod tests;
