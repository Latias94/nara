use std::{collections::BTreeMap, collections::VecDeque, sync::Arc};

use thiserror::Error;

use crate::{UiFrame, UiFrameRevision, UiLimits, UiLimitsError, UiPresentationReceipt};

/// Host-allocated, non-reused identifier for one UI surface lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiSurfaceId(u64);

impl UiSurfaceId {
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Event-time route stamp captured by the platform adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiRouteStamp {
    surface_id: UiSurfaceId,
    presentation_epoch: u64,
}

impl UiRouteStamp {
    #[must_use]
    pub const fn new(surface_id: UiSurfaceId, presentation_epoch: u64) -> Self {
        Self {
            surface_id,
            presentation_epoch,
        }
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn presentation_epoch(self) -> u64 {
        self.presentation_epoch
    }
}

/// Exact retained frame/receipt pair selected for one input route.
#[derive(Clone, Debug)]
pub struct UiRouteTarget {
    frame: Arc<UiFrame>,
    receipt: UiPresentationReceipt,
}

impl UiRouteTarget {
    #[must_use]
    pub fn frame(&self) -> &Arc<UiFrame> {
        &self.frame
    }

    #[must_use]
    pub const fn receipt(&self) -> UiPresentationReceipt {
        self.receipt
    }

    #[must_use]
    pub fn revision(&self) -> UiFrameRevision {
        self.frame.revision()
    }
}

#[derive(Clone, Debug)]
struct PresentedPair {
    frame: Arc<UiFrame>,
    receipt: UiPresentationReceipt,
}

#[derive(Clone, Debug, Default)]
struct SurfaceHistory {
    entries: VecDeque<PresentedPair>,
}

/// Bounded authority journal retaining the current and two prior submitted frames per surface.
#[derive(Clone, Debug)]
pub struct UiFrameJournal {
    limits: UiLimits,
    active: BTreeMap<UiSurfaceId, SurfaceHistory>,
    retired: VecDeque<UiRouteStamp>,
}

impl UiFrameJournal {
    pub fn with_limits(limits: UiLimits) -> Result<Self, UiJournalError> {
        limits.validate()?;
        Ok(Self {
            limits,
            active: BTreeMap::new(),
            retired: VecDeque::new(),
        })
    }

    /// Records a receipt after the caller has accepted and submitted the exact frame packet.
    pub fn submit(
        &mut self,
        frame: Arc<UiFrame>,
        presentation_epoch: u64,
    ) -> Result<UiPresentationReceipt, UiJournalError> {
        if presentation_epoch == 0 {
            return Err(UiJournalError::ZeroPresentationEpoch);
        }
        let environment = frame.environment();
        let surface_id = environment.surface_id();
        if !self.active.contains_key(&surface_id) && self.active.len() == self.limits.max_surfaces {
            return Err(UiJournalError::SurfaceLimitExceeded {
                maximum: self.limits.max_surfaces,
            });
        }
        let history = self.active.entry(surface_id).or_default();
        if let Some(latest) = history.entries.front()
            && presentation_epoch <= latest.receipt.presentation_epoch
        {
            return Err(UiJournalError::NonMonotonicPresentationEpoch {
                previous: latest.receipt.presentation_epoch,
                candidate: presentation_epoch,
            });
        }
        let receipt = UiPresentationReceipt {
            surface_id,
            frame_revision: frame.revision(),
            route_revision: environment.route_revision(),
            presentation_revision: environment.presentation_revision(),
            presentation_epoch,
        };
        history.entries.push_front(PresentedPair { frame, receipt });
        history.entries.truncate(3);
        Ok(receipt)
    }

    pub fn resolve(&self, route: UiRouteStamp) -> Result<UiRouteTarget, UiJournalError> {
        if route.presentation_epoch == 0 {
            return Err(UiJournalError::ZeroPresentationEpoch);
        }
        if let Some(history) = self.active.get(&route.surface_id) {
            if let Some(pair) = history
                .entries
                .iter()
                .find(|pair| pair.receipt.presentation_epoch == route.presentation_epoch)
            {
                verify_pair(pair)?;
                return Ok(UiRouteTarget {
                    frame: Arc::clone(&pair.frame),
                    receipt: pair.receipt,
                });
            }
            if history.entries.is_empty() {
                return Err(UiJournalError::CompiledButNotPresented {
                    surface: route.surface_id,
                });
            }
            return Err(UiJournalError::FrameRetired {
                surface: route.surface_id,
                presentation_epoch: route.presentation_epoch,
            });
        }
        if self.retired.contains(&route) {
            Err(UiJournalError::SurfaceRetired {
                surface: route.surface_id,
                presentation_epoch: route.presentation_epoch,
            })
        } else {
            Err(UiJournalError::CompiledButNotPresented {
                surface: route.surface_id,
            })
        }
    }

    /// Retires all presentation authority for a surface while retaining bounded rejection facts.
    pub fn retire_surface(&mut self, surface_id: UiSurfaceId) -> bool {
        let Some(history) = self.active.remove(&surface_id) else {
            return false;
        };
        for pair in history.entries {
            self.retired.push_front(UiRouteStamp::new(
                surface_id,
                pair.receipt.presentation_epoch,
            ));
        }
        self.retired.truncate(self.limits.max_surfaces * 3);
        true
    }
}

impl Default for UiFrameJournal {
    fn default() -> Self {
        Self::with_limits(UiLimits::default()).expect("default UI limits are valid")
    }
}

fn verify_pair(pair: &PresentedPair) -> Result<(), UiJournalError> {
    let environment = pair.frame.environment();
    let receipt = pair.receipt;
    if receipt.surface_id != environment.surface_id()
        || receipt.frame_revision != pair.frame.revision()
        || receipt.route_revision != environment.route_revision()
        || receipt.presentation_revision != environment.presentation_revision()
    {
        return Err(UiJournalError::ReceiptMismatch {
            surface: receipt.surface_id,
            frame: receipt.frame_revision,
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiJournalError {
    #[error(transparent)]
    Limits(#[from] UiLimitsError),
    #[error("UI presentation epoch must be non-zero")]
    ZeroPresentationEpoch,
    #[error("UI presentation epoch {candidate} does not follow {previous}")]
    NonMonotonicPresentationEpoch { previous: u64, candidate: u64 },
    #[error("UI presentation journal exceeds {maximum} active surfaces")]
    SurfaceLimitExceeded { maximum: usize },
    #[error("surface {surface:?} has no submitted frame for this route")]
    CompiledButNotPresented { surface: UiSurfaceId },
    #[error("surface {surface:?} presentation epoch {presentation_epoch} has retired")]
    FrameRetired {
        surface: UiSurfaceId,
        presentation_epoch: u64,
    },
    #[error("surface {surface:?} presentation epoch {presentation_epoch} was retired")]
    SurfaceRetired {
        surface: UiSurfaceId,
        presentation_epoch: u64,
    },
    #[error("receipt for surface {surface:?} frame {frame:?} does not match its retained frame")]
    ReceiptMismatch {
        surface: UiSurfaceId,
        frame: UiFrameRevision,
    },
}
