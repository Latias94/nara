//! Original CPU temporal lineages and exact candidate-local history declarations.

mod state;

use std::{
    marker::PhantomData,
    mem::size_of,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use nara_render::{RenderFramePacket, RenderFrameSourceGeneration, RenderViewId};

use crate::{
    WgpuFeatureDeclaration, WgpuProgramBuffer, WgpuProgramBufferOutput, WgpuProgramTexture,
    WgpuProgramTextureDescriptor, WgpuProgramTextureOutput, WgpuRenderFeature,
    WgpuRenderProgramError,
    graph::{BufferMeaning, ResourceDescriptor, ResourceVersion},
    program::ProgramResourceStorage,
};

pub(crate) use state::{History, HistoryBasis, HistoryError, HistoryKey};

static NEXT_HISTORY_LINEAGE: AtomicU64 = AtomicU64::new(1);
pub(crate) const MAX_HISTORY_SLOTS: usize = 512;
pub(crate) const MAX_HISTORY_LINEAGES: usize = 64;

/// One original CPU history clock, independent of a target, native Host or slot membership.
///
/// Keep this value with the temporal renderer and clone it into its immutable captures. Resetting
/// slots or replacing effects does not start another lineage or clear its successful watermark.
/// Each clone freezes its semantic revision; cloning an old capture cannot refresh that revision.
/// The domain must clone the current owner atomically with its semantic sample, using its own
/// synchronization rather than relying on a later policy admission or Host preparation.
#[derive(Clone, Debug)]
pub struct WgpuHistoryLineage(pub(crate) Arc<HistoryLineage>, u64);

#[derive(Debug)]
pub(crate) struct HistoryLineage {
    pub(crate) id: u64,
    pub(crate) source: RenderFrameSourceGeneration,
    first_backend: AtomicU64,
    revision: AtomicU64,
}

impl WgpuHistoryLineage {
    pub fn new(source: RenderFrameSourceGeneration) -> Result<Self, WgpuRenderProgramError> {
        let id = NEXT_HISTORY_LINEAGE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| WgpuRenderProgramError::IdentityExhausted)?;
        Ok(Self(
            Arc::new(HistoryLineage {
                id,
                source,
                first_backend: AtomicU64::new(0),
                revision: AtomicU64::new(0),
            }),
            0,
        ))
    }

    /// Cuts semantic continuity without replacing the lineage, its Views or successful watermark.
    ///
    /// Mutate the domain's original current owner before taking subsequent semantic samples.
    /// Previously cloned captures remain stale, even if first admitted or prepared after this cut;
    /// only clones of the updated owner carry the new revision. Stale aliases cannot advance or
    /// resample the current revision and return [`WgpuRenderProgramError::StaleHistory`]. Revision
    /// exhaustion returns [`WgpuRenderProgramError::IdentityExhausted`] without changing the owner.
    ///
    /// This does not empty any Host history slots. Call [`crate::WgpuRenderHost::reset_history`]
    /// explicitly where predecessor storage must also be invalidated. Ordinary slot reset remains
    /// local to that Host realization and does not invalidate all original captures.
    ///
    /// This CPU cut is not rollback or a cross-thread linearized native submission/publication
    /// transaction. Hosts check the frozen revision before native work and at final history
    /// validation; a racing cut cannot undo submitted work or retract already published output.
    /// A cut after final validation may still be followed by that transaction's publication.
    /// Strict ordering between a cut and the whole Host transaction requires domain synchronization.
    pub fn discontinue(&mut self) -> Result<(), WgpuRenderProgramError> {
        self.validate_current()?;
        let next = self
            .1
            .checked_add(1)
            .ok_or(WgpuRenderProgramError::IdentityExhausted)?;
        self.0
            .revision
            .compare_exchange(self.1, next, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| WgpuRenderProgramError::StaleHistory)?;
        self.1 = next;
        Ok(())
    }

    pub(crate) fn validate_current(&self) -> Result<(), WgpuRenderProgramError> {
        if self.0.revision.load(Ordering::Acquire) != self.1 {
            return Err(WgpuRenderProgramError::StaleHistory);
        }
        Ok(())
    }

    #[must_use]
    pub fn source_generation(&self) -> RenderFrameSourceGeneration {
        self.0.source
    }

    pub(crate) fn empty_reason(&self, backend: u64) -> WgpuEmptyHistoryReason {
        let first = self
            .0
            .first_backend
            .compare_exchange(0, backend, Ordering::AcqRel, Ordering::Acquire)
            .unwrap_or_else(|original| original);
        if first == 0 || first == backend {
            WgpuEmptyHistoryReason::Uninitialized
        } else {
            WgpuEmptyHistoryReason::DeviceRecreated
        }
    }
}

/// Why a declared temporal input has no predecessor in this Host realization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WgpuEmptyHistoryReason {
    Uninitialized,
    Reset,
    DeviceRecreated,
}

/// CPU ownership of one View's temporal membership, independent of extraction visibility.
///
/// Retain this owner with the actual View topology and clone it into original captures. Temporary
/// omission does not retire it. Dropping its final original declaration/input/candidate holder
/// permits that View's history rows to retire even when an effect declaration is shared elsewhere.
#[derive(Clone, Debug)]
pub struct WgpuHistoryView {
    id: RenderViewId,
    lifetime: Arc<()>,
}

impl WgpuHistoryView {
    #[must_use]
    pub fn new(id: RenderViewId) -> Self {
        Self {
            id,
            lifetime: Arc::new(()),
        }
    }

    #[must_use]
    pub const fn id(&self) -> RenderViewId {
        self.id
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HistorySlotLease {
    pub(crate) key: HistoryKey,
    feature: Weak<()>,
    view: Weak<()>,
}

impl HistorySlotLease {
    pub(crate) fn is_live(&self) -> bool {
        self.feature.strong_count() != 0 && self.view.strong_count() != 0
    }

    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.key == other.key
            && Weak::ptr_eq(&self.feature, &other.feature)
            && Weak::ptr_eq(&self.view, &other.view)
    }

    fn new<F: WgpuRenderFeature>(
        view: &WgpuHistoryView,
        feature: &WgpuFeatureDeclaration<F>,
        slot: u64,
    ) -> Self {
        Self {
            key: HistoryKey {
                view: view.id,
                feature: feature.id(),
                slot,
            },
            feature: feature.lifetime(),
            view: Arc::downgrade(&view.lifetime),
        }
    }
}

/// A CPU-only texture slot in one candidate's complete temporal declaration.
///
/// This token contains no native output. The build context supplies its already pinned exact
/// predecessor; a token from another preparation cannot select this candidate's history.
pub struct WgpuHistoryTexture<S: 'static> {
    owner: Arc<()>,
    lease: HistorySlotLease,
    key: HistoryKey,
    descriptor: WgpuProgramTextureDescriptor,
    semantic: PhantomData<fn() -> S>,
}

impl<S: 'static> Clone for WgpuHistoryTexture<S> {
    fn clone(&self) -> Self {
        Self {
            owner: Arc::clone(&self.owner),
            lease: self.lease.clone(),
            key: self.key,
            descriptor: self.descriptor.clone(),
            semantic: PhantomData,
        }
    }
}

impl<S: 'static> std::fmt::Debug for WgpuHistoryTexture<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuHistoryTexture")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl<S: 'static> WgpuHistoryTexture<S> {
    /// Selects the exact new local version for this slot. Complete binding is checked before
    /// native work; output order cannot change the slot selected by this declaration.
    #[must_use]
    pub fn bind(&self, output: &WgpuProgramTexture<S>) -> WgpuHistoryBinding {
        WgpuHistoryBinding {
            owner: Arc::clone(&self.owner),
            key: self.key,
            version: output.version.clone(),
            descriptor: self.descriptor.resource::<S>(),
        }
    }

    /// An explicit reset request, including the descriptor eligible after the reset.
    #[must_use]
    pub fn reset(&self) -> WgpuHistoryReset {
        WgpuHistoryReset {
            key: self.key,
            descriptor: self.descriptor.resource::<S>(),
            lease: self.lease.clone(),
        }
    }
}

/// Explicit association between a declared slot and its candidate-local output version.
#[derive(Clone, Debug)]
pub struct WgpuHistoryBinding {
    owner: Arc<()>,
    pub(crate) key: HistoryKey,
    pub(crate) version: ResourceVersion,
    descriptor: ResourceDescriptor,
}

/// A requested reset cannot mint output or clear the lineage's successful-capture watermark.
#[derive(Clone, Debug)]
pub struct WgpuHistoryReset {
    pub(crate) key: HistoryKey,
    pub(crate) descriptor: ResourceDescriptor,
    pub(crate) lease: HistorySlotLease,
}

impl WgpuHistoryReset {
    /// Explicitly selects a texture descriptor after resize or another declared discontinuity.
    /// This requests invalidation only; it cannot initialize or publish that slot.
    #[must_use]
    pub fn texture<S: 'static, F: WgpuRenderFeature>(
        view: &WgpuHistoryView,
        feature: &WgpuFeatureDeclaration<F>,
        slot: u64,
        descriptor: WgpuProgramTextureDescriptor,
    ) -> Self {
        let lease = HistorySlotLease::new(view, feature, slot);
        Self {
            key: lease.key,
            descriptor: descriptor.resource::<S>(),
            lease,
        }
    }

    #[must_use]
    pub fn buffer<S: 'static, F: WgpuRenderFeature>(
        view: &WgpuHistoryView,
        feature: &WgpuFeatureDeclaration<F>,
        slot: u64,
        descriptor: wgpu::BufferDescriptor<'static>,
    ) -> Self {
        let lease = HistorySlotLease::new(view, feature, slot);
        Self {
            key: lease.key,
            descriptor: ResourceDescriptor::Buffer {
                native: descriptor,
                meaning: BufferMeaning::of::<S>(),
            },
            lease,
        }
    }
}

/// CPU-only whole-buffer history slot, with the same complete publication rules as a texture.
pub struct WgpuHistoryBuffer<S: 'static> {
    owner: Arc<()>,
    lease: HistorySlotLease,
    key: HistoryKey,
    descriptor: wgpu::BufferDescriptor<'static>,
    semantic: PhantomData<fn() -> S>,
}

impl<S: 'static> Clone for WgpuHistoryBuffer<S> {
    fn clone(&self) -> Self {
        Self {
            owner: Arc::clone(&self.owner),
            lease: self.lease.clone(),
            key: self.key,
            descriptor: self.descriptor.clone(),
            semantic: PhantomData,
        }
    }
}

impl<S: 'static> WgpuHistoryBuffer<S> {
    fn resource(&self) -> ResourceDescriptor {
        ResourceDescriptor::Buffer {
            native: self.descriptor.clone(),
            meaning: BufferMeaning::of::<S>(),
        }
    }

    #[must_use]
    pub fn bind(&self, output: &WgpuProgramBuffer<S>) -> WgpuHistoryBinding {
        WgpuHistoryBinding {
            owner: Arc::clone(&self.owner),
            key: self.key,
            version: output.version.clone(),
            descriptor: self.resource(),
        }
    }

    #[must_use]
    pub fn reset(&self) -> WgpuHistoryReset {
        WgpuHistoryReset {
            key: self.key,
            descriptor: self.resource(),
            lease: self.lease.clone(),
        }
    }
}

/// CPU declarations collected during binding, before pinning the complete predecessor basis.
pub(crate) struct HistoryDeclarations {
    owner: Arc<()>,
    pub(crate) entries: Vec<(HistoryKey, ResourceDescriptor)>,
    leases: Vec<HistorySlotLease>,
}

impl Default for HistoryDeclarations {
    fn default() -> Self {
        Self {
            owner: Arc::new(()),
            entries: Vec::new(),
            leases: Vec::new(),
        }
    }
}

impl HistoryDeclarations {
    pub(crate) fn buffer<S: 'static, F: WgpuRenderFeature>(
        &mut self,
        packet: &RenderFramePacket,
        view: &WgpuHistoryView,
        feature: &WgpuFeatureDeclaration<F>,
        slot: u64,
        descriptor: wgpu::BufferDescriptor<'static>,
    ) -> Result<WgpuHistoryBuffer<S>, WgpuRenderProgramError> {
        if !packet
            .views()
            .iter()
            .any(|candidate| candidate.id == view.id)
        {
            return Err(WgpuRenderProgramError::feature(
                "history requires its original captured View",
            ));
        }
        let lease = HistorySlotLease::new(view, feature, slot);
        let key = lease.key;
        if self.entries.len() == MAX_HISTORY_SLOTS {
            return Err(WgpuRenderProgramError::HistoryCapacity);
        }
        if self.entries.iter().any(|(existing, _)| *existing == key) {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        self.entries
            .try_reserve(1)
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        self.leases
            .try_reserve(1)
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        self.entries.push((
            key,
            ResourceDescriptor::Buffer {
                native: descriptor.clone(),
                meaning: BufferMeaning::of::<S>(),
            },
        ));
        self.leases.push(lease.clone());
        Ok(WgpuHistoryBuffer {
            owner: Arc::clone(&self.owner),
            lease,
            key,
            descriptor,
            semantic: PhantomData,
        })
    }

    pub(crate) fn texture<S: 'static, F: WgpuRenderFeature>(
        &mut self,
        packet: &RenderFramePacket,
        view: &WgpuHistoryView,
        feature: &WgpuFeatureDeclaration<F>,
        slot: u64,
        descriptor: WgpuProgramTextureDescriptor,
    ) -> Result<WgpuHistoryTexture<S>, WgpuRenderProgramError> {
        if !packet
            .views()
            .iter()
            .any(|candidate| candidate.id == view.id)
        {
            return Err(WgpuRenderProgramError::feature(
                "history requires its original captured View",
            ));
        }
        let lease = HistorySlotLease::new(view, feature, slot);
        let key = lease.key;
        if self.entries.len() == MAX_HISTORY_SLOTS {
            return Err(WgpuRenderProgramError::HistoryCapacity);
        }
        if self.entries.iter().any(|(existing, _)| *existing == key) {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        self.entries
            .try_reserve(1)
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        self.leases
            .try_reserve(1)
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        self.entries.push((key, descriptor.resource::<S>()));
        self.leases.push(lease.clone());
        Ok(WgpuHistoryTexture {
            owner: Arc::clone(&self.owner),
            lease,
            key,
            descriptor,
            semantic: PhantomData,
        })
    }

    pub(crate) fn pin(
        self,
        history: &HistoryRealization,
        capture: u64,
    ) -> Result<PreparedHistory, WgpuRenderProgramError> {
        let basis = history.state.prepare(capture, &self.entries)?;
        let mut leases = Vec::new();
        leases
            .try_reserve_exact(MAX_HISTORY_SLOTS)
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        leases.extend(history.leases.iter().cloned());
        for lease in self.leases {
            if let Some(existing) = leases.iter().find(|existing| existing.key == lease.key) {
                if !existing.matches(&lease) {
                    return Err(WgpuRenderProgramError::HistoryDeclaration);
                }
            } else {
                leases.push(lease);
            }
        }
        Ok(PreparedHistory {
            owner: self.owner,
            basis,
            declarations: self.entries,
            empty_reason: history.empty_reason,
            leases,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub(crate) struct PreparedHistory {
    owner: Arc<()>,
    pub(crate) basis: HistoryBasis<ProgramResourceStorage>,
    pub(crate) declarations: Vec<(HistoryKey, ResourceDescriptor)>,
    pub(crate) empty_reason: WgpuEmptyHistoryReason,
    pub(crate) leases: Vec<HistorySlotLease>,
}

impl PreparedHistory {
    pub(crate) fn previous_buffer<S: 'static>(
        &self,
        slot: &WgpuHistoryBuffer<S>,
    ) -> Result<Option<WgpuProgramBufferOutput<S>>, WgpuRenderProgramError> {
        if !Arc::ptr_eq(&self.owner, &slot.owner) {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        Ok(self
            .basis
            .previous(slot.key)?
            .map(|storage| WgpuProgramBufferOutput::new(Arc::clone(storage))))
    }

    pub(crate) fn previous_texture<S: 'static>(
        &self,
        slot: &WgpuHistoryTexture<S>,
    ) -> Result<Option<WgpuProgramTextureOutput<S>>, WgpuRenderProgramError> {
        if !Arc::ptr_eq(&self.owner, &slot.owner) {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        Ok(self
            .basis
            .previous(slot.key)?
            .map(|storage| WgpuProgramTextureOutput::new(Arc::clone(storage))))
    }

    pub(crate) fn validate_bindings(
        &self,
        program: &crate::WgpuRenderProgram,
        bindings: &[WgpuHistoryBinding],
    ) -> Result<(), WgpuRenderProgramError> {
        if bindings.len() != self.declarations.len() {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        for (index, binding) in bindings.iter().enumerate() {
            if !Arc::ptr_eq(&self.owner, &binding.owner)
                || bindings[..index]
                    .iter()
                    .any(|previous| previous.key == binding.key)
                || !self.declarations.iter().any(|(key, descriptor)| {
                    *key == binding.key && *descriptor == binding.descriptor
                })
                || !program.0.graph.outputs().contains(&binding.version)
                || program.0.graph.resource(&binding.version)?.desc != binding.descriptor
                || program.0.graph.resource(&binding.version)?.is_imported()
            {
                return Err(WgpuRenderProgramError::HistoryDeclaration);
            }
        }
        Ok(())
    }

    pub(crate) fn metadata_bytes(&self) -> Result<usize, WgpuRenderProgramError> {
        size_of::<Self>()
            .checked_add(
                self.declarations
                    .len()
                    .checked_mul(
                        size_of::<(HistoryKey, ResourceDescriptor)>()
                            + size_of::<WgpuHistoryBinding>()
                            + 2 * size_of::<Arc<ProgramResourceStorage>>(),
                    )
                    .ok_or(WgpuRenderProgramError::HistoryCapacity)?,
            )
            .and_then(|bytes| {
                self.basis
                    .metadata_bytes()
                    .ok()
                    .and_then(|basis| bytes.checked_add(basis))
            })
            .and_then(|bytes| {
                self.leases
                    .capacity()
                    .checked_mul(size_of::<HistorySlotLease>())
                    .and_then(|leases| bytes.checked_add(leases))
            })
            .ok_or(WgpuRenderProgramError::HistoryCapacity)
    }

    pub(crate) fn empty_texture_reason<S: 'static>(
        &self,
        slot: &WgpuHistoryTexture<S>,
    ) -> Result<Option<WgpuEmptyHistoryReason>, WgpuRenderProgramError> {
        if !Arc::ptr_eq(&self.owner, &slot.owner) {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        if self.basis.previous(slot.key)?.is_some() {
            return Ok(None);
        }
        Ok(Some(if self.basis.was_reset(slot.key)? {
            WgpuEmptyHistoryReason::Reset
        } else {
            self.empty_reason
        }))
    }

    pub(crate) fn empty_buffer_reason<S: 'static>(
        &self,
        slot: &WgpuHistoryBuffer<S>,
    ) -> Result<Option<WgpuEmptyHistoryReason>, WgpuRenderProgramError> {
        if !Arc::ptr_eq(&self.owner, &slot.owner) {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        if self.basis.previous(slot.key)?.is_some() {
            return Ok(None);
        }
        Ok(Some(if self.basis.was_reset(slot.key)? {
            WgpuEmptyHistoryReason::Reset
        } else {
            self.empty_reason
        }))
    }
}

pub(crate) struct HistoryRealization {
    pub(crate) lineage: Weak<HistoryLineage>,
    pub(crate) id: u64,
    pub(crate) state: History<ProgramResourceStorage>,
    pub(crate) empty_reason: WgpuEmptyHistoryReason,
    pub(crate) _metadata: crate::output::OutputReservation,
    pub(crate) leases: Vec<HistorySlotLease>,
}

impl HistoryRealization {
    pub(crate) fn new(
        lineage: &WgpuHistoryLineage,
        backend: u64,
        metadata: crate::output::OutputReservation,
    ) -> Self {
        Self {
            lineage: Arc::downgrade(&lineage.0),
            id: lineage.0.id,
            state: History::new(MAX_HISTORY_SLOTS),
            empty_reason: lineage.empty_reason(backend),
            _metadata: metadata,
            leases: Vec::new(),
        }
    }
}

impl From<HistoryError> for WgpuRenderProgramError {
    fn from(error: HistoryError) -> Self {
        match error {
            HistoryError::StaleBasis | HistoryError::ForeignBasis => Self::StaleHistory,
            HistoryError::RepeatedCapture => Self::HistoryAlreadyConsumed,
            HistoryError::Capacity | HistoryError::Overflow => Self::HistoryCapacity,
            HistoryError::Allocation => Self::ResourceCapacity,
            _ => Self::HistoryDeclaration,
        }
    }
}
