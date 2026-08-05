use nara_ui_kernel::{UiActionId, UiActivationSource, UiKey};
use thiserror::Error;

const HARD_MAX_INTENTS: usize = 64;

/// A semantic request that must be mapped by the owning runtime or tooling domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiIntent {
    Activate {
        target: UiKey,
        action: UiActionId,
        source: UiActivationSource,
    },
}

impl UiIntent {
    #[must_use]
    pub const fn target(&self) -> &UiKey {
        match self {
            Self::Activate { target, .. } => target,
        }
    }
}

/// Bounded intent collection for one adapter-owned dispatch pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiIntentBatch {
    maximum: usize,
    intents: Vec<UiIntent>,
}

impl UiIntentBatch {
    pub fn with_maximum(maximum: usize) -> Result<Self, UiIntentBatchError> {
        if maximum == 0 || maximum > HARD_MAX_INTENTS {
            return Err(UiIntentBatchError::InvalidMaximum {
                actual: maximum,
                hard_maximum: HARD_MAX_INTENTS,
            });
        }
        Ok(Self {
            maximum,
            intents: Vec::new(),
        })
    }

    #[must_use]
    pub fn intents(&self) -> &[UiIntent] {
        &self.intents
    }

    pub fn clear(&mut self) {
        self.intents.clear();
    }

    pub(crate) fn try_extend(
        &mut self,
        intents: impl IntoIterator<Item = UiIntent>,
    ) -> Result<(), UiIntentBatchError> {
        let intents = intents.into_iter().collect::<Vec<_>>();
        let actual = self.intents.len().saturating_add(intents.len());
        if actual > self.maximum {
            return Err(UiIntentBatchError::LimitExceeded {
                actual,
                maximum: self.maximum,
            });
        }
        self.intents.extend(intents);
        Ok(())
    }
}

impl Default for UiIntentBatch {
    fn default() -> Self {
        Self::with_maximum(HARD_MAX_INTENTS).expect("the hard UI intent limit is valid")
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiIntentBatchError {
    #[error("UI intent maximum is {actual}; expected 1..={hard_maximum}")]
    InvalidMaximum { actual: usize, hard_maximum: usize },
    #[error("UI intent batch would contain {actual} entries; maximum is {maximum}")]
    LimitExceeded { actual: usize, maximum: usize },
}
