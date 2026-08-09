use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use thiserror::Error;

use crate::{UiFrame, UiFrameRevision, UiLimits, UiLimitsError, UiPresentationReceipt, UiRevision};

#[derive(Debug, Default)]
struct UiFrameJournalSubmissionIdentity;

/// Exclusive capability for staging presented frames in one hosted journal lineage.
///
/// Ordinary journals created through [`UiFrameJournal::with_limits`] cannot submit frames. A
/// first-party presentation Host obtains this capability together with the journal, retains it
/// outside product-facing state, and may stage only clones of that exact journal lineage.
#[derive(Debug)]
pub struct UiFrameJournalAuthority {
    identity: Arc<UiFrameJournalSubmissionIdentity>,
}

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
    slot_generation: UiRevision,
    route_revision: UiRevision,
    geometry_generation: UiRevision,
    presentation_epoch: u64,
}

impl UiRouteStamp {
    #[must_use]
    pub const fn new(
        surface_id: UiSurfaceId,
        slot_generation: UiRevision,
        route_revision: UiRevision,
        geometry_generation: UiRevision,
        presentation_epoch: u64,
    ) -> Self {
        Self {
            surface_id,
            slot_generation,
            route_revision,
            geometry_generation,
            presentation_epoch,
        }
    }

    #[must_use]
    pub const fn from_receipt(receipt: UiPresentationReceipt) -> Self {
        Self::new(
            receipt.surface_id(),
            receipt.slot_generation(),
            receipt.route_revision(),
            receipt.geometry_generation(),
            receipt.presentation_epoch(),
        )
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn slot_generation(self) -> UiRevision {
        self.slot_generation
    }

    #[must_use]
    pub const fn route_revision(self) -> UiRevision {
        self.route_revision
    }

    #[must_use]
    pub const fn geometry_generation(self) -> UiRevision {
        self.geometry_generation
    }

    #[must_use]
    pub const fn presentation_epoch(self) -> u64 {
        self.presentation_epoch
    }
}

/// Exact evidence that one Surface Slot generation was retired by its Host owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiSurfaceRetirementToken {
    surface_id: UiSurfaceId,
    slot_generation: UiRevision,
    retirement_generation: UiRevision,
}

impl UiSurfaceRetirementToken {
    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn slot_generation(self) -> UiRevision {
        self.slot_generation
    }

    #[must_use]
    pub const fn retirement_generation(self) -> UiRevision {
        self.retirement_generation
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

#[derive(Clone, Debug)]
struct SurfaceHistory {
    slot_generation: UiRevision,
    entries: VecDeque<PresentedPair>,
}

impl SurfaceHistory {
    const fn new(slot_generation: UiRevision) -> Self {
        Self {
            slot_generation,
            entries: VecDeque::new(),
        }
    }
}

/// Bounded authority journal retaining the current and two prior submitted frames per surface.
#[derive(Clone, Debug)]
pub struct UiFrameJournal {
    submission_identity: Arc<UiFrameJournalSubmissionIdentity>,
    limits: UiLimits,
    active: BTreeMap<UiSurfaceId, SurfaceHistory>,
    retired_surfaces: BTreeSet<UiSurfaceId>,
    retired: VecDeque<UiRouteStamp>,
}

impl UiFrameJournal {
    pub fn with_limits(limits: UiLimits) -> Result<Self, UiJournalError> {
        limits.validate()?;
        Ok(Self {
            submission_identity: Arc::new(UiFrameJournalSubmissionIdentity),
            limits,
            active: BTreeMap::new(),
            retired_surfaces: BTreeSet::new(),
            retired: VecDeque::new(),
        })
    }

    pub(crate) fn new_hosted(
        limits: UiLimits,
    ) -> Result<(Self, UiFrameJournalAuthority), UiJournalError> {
        let journal = Self::with_limits(limits)?;
        let authority = UiFrameJournalAuthority {
            identity: Arc::clone(&journal.submission_identity),
        };
        Ok((journal, authority))
    }

    /// Registers one installed Surface Slot even when it has not presented a frame yet.
    pub fn install_surface(
        &mut self,
        surface_id: UiSurfaceId,
        slot_generation: u64,
    ) -> Result<(), UiJournalError> {
        let slot_generation =
            UiRevision::new(slot_generation).ok_or(UiJournalError::ZeroSlotGeneration)?;
        if self.active.contains_key(&surface_id) {
            return Err(UiJournalError::SurfaceAlreadyInstalled {
                surface: surface_id,
            });
        }
        if self.retired_surfaces.contains(&surface_id) {
            return Err(UiJournalError::SurfaceIdentityRetired {
                surface: surface_id,
            });
        }
        if self.active.len() == self.limits.max_surfaces {
            return Err(UiJournalError::SurfaceLimitExceeded {
                maximum: self.limits.max_surfaces,
            });
        }
        self.active
            .insert(surface_id, SurfaceHistory::new(slot_generation));
        Ok(())
    }

    /// Advances one installed surface to a new Host-owned Slot generation.
    ///
    /// Every receipt from the previous Slot is retired immediately. The new Slot remains
    /// installed even before its first frame is presented, so a later Host retirement can still
    /// issue an exact token for that lifetime.
    pub fn advance_surface_slot(
        &mut self,
        surface_id: UiSurfaceId,
        previous_slot_generation: u64,
        next_slot_generation: u64,
    ) -> Result<(), UiJournalError> {
        let previous_slot_generation =
            UiRevision::new(previous_slot_generation).ok_or(UiJournalError::ZeroSlotGeneration)?;
        let next_slot_generation =
            UiRevision::new(next_slot_generation).ok_or(UiJournalError::ZeroSlotGeneration)?;
        let history =
            self.active
                .get_mut(&surface_id)
                .ok_or(UiJournalError::SurfaceNotInstalled {
                    surface: surface_id,
                })?;
        if history.slot_generation != previous_slot_generation {
            return Err(UiJournalError::SlotGenerationMismatch {
                surface: surface_id,
                active: history.slot_generation,
                expected: previous_slot_generation,
            });
        }
        if next_slot_generation <= previous_slot_generation {
            return Err(UiJournalError::SlotGenerationDidNotAdvance {
                surface: surface_id,
                previous: previous_slot_generation,
                candidate: next_slot_generation,
            });
        }

        for pair in history.entries.drain(..) {
            self.retired
                .push_front(UiRouteStamp::from_receipt(pair.receipt));
        }
        self.retired.truncate(self.limits.max_surfaces * 3);
        history.slot_generation = next_slot_generation;
        Ok(())
    }

    /// Records a receipt after the first-party presentation owner has accepted and submitted the
    /// exact frame packet.
    fn submit(
        &mut self,
        authority: &UiFrameJournalAuthority,
        frame: Arc<UiFrame>,
        presentation_epoch: u64,
    ) -> Result<UiPresentationReceipt, UiJournalError> {
        if !Arc::ptr_eq(&self.submission_identity, &authority.identity) {
            return Err(UiJournalError::SubmissionAuthorityMismatch);
        }
        if presentation_epoch == 0 {
            return Err(UiJournalError::ZeroPresentationEpoch);
        }
        let environment = frame.environment();
        let surface_id = environment.surface_id();
        let history =
            self.active
                .get_mut(&surface_id)
                .ok_or(UiJournalError::SurfaceNotInstalled {
                    surface: surface_id,
                })?;
        if environment.slot_generation() != history.slot_generation {
            return Err(UiJournalError::SlotGenerationMismatch {
                surface: surface_id,
                active: history.slot_generation,
                expected: environment.slot_generation(),
            });
        }
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
            generations: environment.generations(),
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
        if self.retired.contains(&route) {
            return Err(UiJournalError::SurfaceRetired {
                surface: route.surface_id,
                presentation_epoch: route.presentation_epoch,
            });
        }
        if self.retired_surfaces.contains(&route.surface_id) {
            return Err(UiJournalError::SurfaceRetired {
                surface: route.surface_id,
                presentation_epoch: route.presentation_epoch,
            });
        }
        if let Some(history) = self.active.get(&route.surface_id) {
            if let Some(pair) = history
                .entries
                .iter()
                .find(|pair| pair.receipt.presentation_epoch == route.presentation_epoch)
            {
                verify_pair(pair)?;
                if pair.receipt.slot_generation() != route.slot_generation
                    || pair.receipt.route_revision() != route.route_revision
                    || pair.receipt.geometry_generation() != route.geometry_generation
                {
                    return Err(UiJournalError::RouteGenerationMismatch {
                        surface: route.surface_id,
                        presentation_epoch: route.presentation_epoch,
                    });
                }
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
        Err(UiJournalError::CompiledButNotPresented {
            surface: route.surface_id,
        })
    }

    /// Returns the newest exact frame/receipt pair submitted for a surface.
    pub fn latest(&self, surface_id: UiSurfaceId) -> Result<UiRouteTarget, UiJournalError> {
        let Some(pair) = self
            .active
            .get(&surface_id)
            .and_then(|history| history.entries.front())
        else {
            return Err(UiJournalError::CompiledButNotPresented {
                surface: surface_id,
            });
        };
        verify_pair(pair)?;
        Ok(UiRouteTarget {
            frame: Arc::clone(&pair.frame),
            receipt: pair.receipt,
        })
    }

    /// Retires all presentation authority for a surface while retaining bounded rejection facts.
    pub fn retire_surface(
        &mut self,
        surface_id: UiSurfaceId,
        retirement_generation: u64,
    ) -> Result<Option<UiSurfaceRetirementToken>, UiJournalError> {
        let retirement_generation = UiRevision::new(retirement_generation)
            .ok_or(UiJournalError::ZeroRetirementGeneration)?;
        if self.retired_surfaces.contains(&surface_id) {
            return Ok(None);
        }
        if self.retired_surfaces.len() == self.limits.max_surface_lifetimes {
            return Err(UiJournalError::SurfaceIdentityLimitExceeded {
                maximum: self.limits.max_surface_lifetimes,
            });
        }
        let Some(history) = self.active.remove(&surface_id) else {
            return Ok(None);
        };
        self.retired_surfaces.insert(surface_id);
        for pair in history.entries {
            self.retired
                .push_front(UiRouteStamp::from_receipt(pair.receipt));
        }
        self.retired.truncate(self.limits.max_surfaces * 3);
        Ok(Some(UiSurfaceRetirementToken {
            surface_id,
            slot_generation: history.slot_generation,
            retirement_generation,
        }))
    }
}

impl UiFrameJournalAuthority {
    /// Stages an exact frame in a clone of the hosted journal before backend submission.
    ///
    /// The capability is instance-bound and intentionally not cloneable. A receipt staged in a
    /// journal clone has no live routing authority until the owning runtime atomically installs the
    /// complete prepared session after backend acceptance.
    pub fn submit(
        &self,
        journal: &mut UiFrameJournal,
        frame: Arc<UiFrame>,
        presentation_epoch: u64,
    ) -> Result<UiPresentationReceipt, UiJournalError> {
        journal.submit(self, frame, presentation_epoch)
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
        || receipt.generations != environment.generations()
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
    #[error("UI presentation journal submission authority does not own this journal lineage")]
    SubmissionAuthorityMismatch,
    #[error("UI surface slot generation must be non-zero")]
    ZeroSlotGeneration,
    #[error("UI surface retirement generation must be non-zero")]
    ZeroRetirementGeneration,
    #[error("UI presentation epoch {candidate} does not follow {previous}")]
    NonMonotonicPresentationEpoch { previous: u64, candidate: u64 },
    #[error("UI presentation journal exceeds {maximum} active surfaces")]
    SurfaceLimitExceeded { maximum: usize },
    #[error("surface {surface:?} is already installed in the presentation journal")]
    SurfaceAlreadyInstalled { surface: UiSurfaceId },
    #[error("surface {surface:?} was retired and cannot be installed again")]
    SurfaceIdentityRetired { surface: UiSurfaceId },
    #[error("UI presentation journal exceeds {maximum} retired surface identities")]
    SurfaceIdentityLimitExceeded { maximum: usize },
    #[error("surface {surface:?} is not installed in the presentation journal")]
    SurfaceNotInstalled { surface: UiSurfaceId },
    #[error(
        "surface {surface:?} active slot generation {active:?} does not match expected generation {expected:?}"
    )]
    SlotGenerationMismatch {
        surface: UiSurfaceId,
        active: UiRevision,
        expected: UiRevision,
    },
    #[error("surface {surface:?} slot generation {candidate:?} does not advance {previous:?}")]
    SlotGenerationDidNotAdvance {
        surface: UiSurfaceId,
        previous: UiRevision,
        candidate: UiRevision,
    },
    #[error("surface {surface:?} has no submitted frame for this route")]
    CompiledButNotPresented { surface: UiSurfaceId },
    #[error("surface {surface:?} presentation epoch {presentation_epoch} has retired")]
    FrameRetired {
        surface: UiSurfaceId,
        presentation_epoch: u64,
    },
    #[error(
        "surface {surface:?} presentation epoch {presentation_epoch} does not match the exact route generations"
    )]
    RouteGenerationMismatch {
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
