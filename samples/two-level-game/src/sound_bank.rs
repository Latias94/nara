//! Game-owned CPU one-shots shared by source import and source-free Runtime content.
//!
//! Every clip is nonempty mono signed PCM16 at 48,000 Hz, with at most 48,000 frames.
//! Samples are exactly `f32::from(pcm16) / 32_768.0`; no resampling or device is involved.

use std::{mem::size_of, sync::Arc};

use nara::{
    asset::{AssetProductRef, ImportedProductId, StableAssetId},
    core::ByteLimit,
    diagnostic::{Diagnostic, DiagnosticCode, SafeSummary},
};

mod runtime;
mod source;
mod wav;

pub use source::SoundBankSettings;

/// Stable identity of the game's sole authored sound bank.
pub const BANK_SOURCE_ID: &str = "6c7d4d42-cb57-4a47-9f16-741293f5aab2";
/// Assets-relative location of the bank's source descriptor.
pub const BANK_PATH: &str = "audio/game.soundbank.json";

const SAMPLE_RATE: u32 = 48_000;
const MAX_CLIP_FRAMES: usize = 48_000;
const MAX_SOURCE_BYTES: usize = 1024;
const MAX_WAV_BYTES: usize = 44 + MAX_CLIP_FRAMES * size_of::<i16>() + 4096;
const MAX_SAMPLE_BYTES: usize = 2 * MAX_CLIP_FRAMES * size_of::<f32>();
const ARC_HEADER_BYTES: usize = 2 * size_of::<usize>();
const MAX_RETAINED_BYTES: usize = size_of::<SoundBank>() + MAX_SAMPLE_BYTES + 2 * ARC_HEADER_BYTES;

/// The exact primary Product used by both levels and every sound-bank operation.
#[must_use]
pub fn bank_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(BANK_SOURCE_ID).expect("fixed valid sound bank source identity"),
        ImportedProductId::PRIMARY,
    )
    .expect("fixed non-nil primary sound bank Product")
}

/// The finite inventory of committed gameplay one-shots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundCue {
    /// A committed pickup.
    Pickup,
    /// Successful entry after travel.
    Travel,
}

/// An immutable complete bank with exact canonical PCM16-derived samples.
///
/// Equality compares the complete Product and both complete sample sequences. Samples are
/// finite, contain only canonical PCM16 values and represent silence with positive zero.
/// Consumers retaining sample allocations must also retain the original domain collection.
#[derive(Debug, PartialEq)]
pub struct SoundBank {
    product: AssetProductRef,
    pickup: Arc<[f32]>,
    travel: Arc<[f32]>,
}

impl SoundBank {
    /// Returns the complete primary Product carried by this bank.
    #[must_use]
    pub const fn product(&self) -> AssetProductRef {
        self.product
    }

    /// Borrows canonical mono samples on the fixed 48,000 Hz clock.
    #[must_use]
    pub fn samples(&self, cue: SoundCue) -> &[f32] {
        match cue {
            SoundCue::Pickup => &self.pickup,
            SoundCue::Travel => &self.travel,
        }
    }

    /// Clones Pickup and Travel storage once at startup, together with its original owner.
    #[must_use]
    pub(crate) fn sample_arcs(&self) -> (Arc<[f32]>, Arc<[f32]>) {
        (Arc::clone(&self.pickup), Arc::clone(&self.travel))
    }

    fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + sample_allocation_bytes(self.pickup.len())
            + sample_allocation_bytes(self.travel.len())
    }
}

/// Stateless source import and tracked build of the game's complete CPU bank.
pub struct SoundBankDomain;

/// Source-independent codec for the complete Product, fixed descriptors and canonical PCM16.
pub struct SoundBankCodec;

/// Models each Arc's two counters and alignment padding, independently of shared Host wrappers.
fn sample_allocation_bytes(frames: usize) -> usize {
    let alignment = std::mem::align_of::<usize>();
    ARC_HEADER_BYTES + (frames * size_of::<f32>()).div_ceil(alignment) * alignment
}

fn byte_limit(bytes: usize) -> ByteLimit {
    ByteLimit::new(bytes).expect("fixed nonzero sound bank byte limit")
}

#[derive(Debug, Clone, Copy)]
enum Rejection {
    Source,
    Settings,
    Identity,
    WavBounds,
    WavShape,
    WavFormat,
    WavEmpty,
    WavFrames,
    RuntimeBounds,
    RuntimeHeader,
    RuntimeVersion,
    RuntimeIdentity,
    RuntimeFormat,
    RuntimeLength,
    CanonicalSamples,
}

impl Rejection {
    fn diagnostic(self) -> Diagnostic {
        let (code, summary) = match self {
            Self::Source => (
                "game.sound-bank.source",
                "Sound bank source requires only bounded pickup and travel Assets-relative paths",
            ),
            Self::Settings => (
                "game.sound-bank.settings",
                "Sound bank settings must be exactly an empty JSON object",
            ),
            Self::Identity => (
                "game.sound-bank.identity",
                "Sound bank metadata must select the game's exact source identity and source kind",
            ),
            Self::WavBounds => (
                "game.sound-bank.wav-bounds",
                "Sound bank WAV exceeds its bounded encoded input allowance",
            ),
            Self::WavShape => (
                "game.sound-bank.wav-shape",
                "Sound bank WAV has invalid RIFF chunks, lengths, padding or trailing bytes",
            ),
            Self::WavFormat => (
                "game.sound-bank.wav-format",
                "Sound bank WAV requires mono 48000 Hz signed PCM16 with consistent format fields",
            ),
            Self::WavEmpty => (
                "game.sound-bank.wav-empty",
                "Every sound bank WAV must contain at least one complete frame",
            ),
            Self::WavFrames => (
                "game.sound-bank.wav-frames",
                "Sound bank WAV exceeds the limit of 48000 frames per cue",
            ),
            Self::RuntimeBounds => (
                "game.sound-bank.runtime-bounds",
                "Encoded Runtime sound bank exceeds its bounded input allowance",
            ),
            Self::RuntimeHeader => (
                "game.sound-bank.runtime-header",
                "Runtime sound bank requires its complete binary format header",
            ),
            Self::RuntimeVersion => (
                "game.sound-bank.runtime-version",
                "Runtime sound bank version is unsupported",
            ),
            Self::RuntimeIdentity => (
                "game.sound-bank.runtime-identity",
                "Runtime sound bank must carry the game's exact complete primary Product",
            ),
            Self::RuntimeFormat => (
                "game.sound-bank.runtime-format",
                "Runtime sound bank requires two mono 48000 Hz little-endian signed PCM16 cues",
            ),
            Self::RuntimeLength => (
                "game.sound-bank.runtime-length",
                "Runtime sound bank requires 1 to 48000 frames per cue and an exact payload length",
            ),
            Self::CanonicalSamples => (
                "game.sound-bank.canonical-samples",
                "Sound bank samples must be finite exact PCM16 values normalized by 32768",
            ),
        };
        Diagnostic::error(
            DiagnosticCode::new(code).expect("fixed valid sound bank diagnostic code"),
            SafeSummary::new(summary).expect("fixed safe sound bank diagnostic summary"),
        )
    }
}

#[cfg(test)]
mod tests;
