//! Preparation of the game's two immutable one-shots, before creating a device callback.

use std::sync::Arc;

use nara::project_host::ProjectDomainAssets;

use super::bank::RetainedSoundBank;

pub(super) const SAMPLE_RATE: u32 = 48_000;
#[cfg(test)]
pub(super) const MAX_CLIP_FRAMES: usize = SAMPLE_RATE as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClipKind {
    Pickup,
    Travel,
}

/// Mono samples on the fixed 48 kHz output clock. Construction may allocate; playback does not.
#[derive(Clone, Debug)]
pub(super) struct PreparedClips {
    pickup: Arc<[f32]>,
    travel: Arc<[f32]>,
    _assets: Option<ProjectDomainAssets>,
}

impl PreparedClips {
    pub(super) fn from_bank(bank: &RetainedSoundBank) -> Self {
        let (pickup, travel) = bank.bank().sample_arcs();
        Self {
            pickup,
            travel,
            _assets: Some(bank.assets().clone()),
        }
    }

    /// Samples supplied directly must already use the fixed sample clock and normalized range.
    #[cfg(test)]
    pub(super) fn new(pickup: Arc<[f32]>, travel: Arc<[f32]>) -> Result<Self, ClipError> {
        validate_samples(ClipKind::Pickup, &pickup)?;
        validate_samples(ClipKind::Travel, &travel)?;
        Ok(Self {
            pickup,
            travel,
            _assets: None,
        })
    }

    pub(super) fn samples(&self, clip: ClipKind) -> &[f32] {
        match clip {
            ClipKind::Pickup => &self.pickup,
            ClipKind::Travel => &self.travel,
        }
    }
}

#[cfg(test)]
#[derive(Debug, thiserror::Error)]
pub(super) enum ClipError {
    #[error("{clip:?} must contain at least one sample")]
    Empty { clip: ClipKind },
    #[error("{clip:?} has {frames} frames; limit is {MAX_CLIP_FRAMES}")]
    TooLong { clip: ClipKind, frames: usize },
    #[error("{clip:?} sample {frame} must be finite and within [-1, 1]")]
    InvalidSample { clip: ClipKind, frame: usize },
}

#[cfg(test)]
fn validate_samples(clip: ClipKind, samples: &[f32]) -> Result<(), ClipError> {
    if samples.is_empty() {
        return Err(ClipError::Empty { clip });
    }
    if samples.len() > MAX_CLIP_FRAMES {
        return Err(ClipError::TooLong {
            clip,
            frames: samples.len(),
        });
    }
    if let Some(frame) = samples
        .iter()
        .position(|sample| !sample.is_finite() || !(-1.0..=1.0).contains(sample))
    {
        return Err(ClipError::InvalidSample { clip, frame });
    }
    Ok(())
}

#[cfg(test)]
#[path = "clip_tests.rs"]
mod tests;
