//! Move-owned policy preparation, distinct from one serial native submission attempt.

use std::{mem::size_of, sync::Arc};

use crate::{
    WgpuRenderHostError, WgpuRenderInput, WgpuRenderProducerId,
    output::{OutputBudget, OutputReservation},
    policy::BuiltPolicy,
};

const MAX_PENDING_CANDIDATES: u64 = 64;
const CANDIDATE_PROGRAM_METADATA: u64 = 64 * 1024 * 1024;
const CANDIDATE_BINDING_METADATA: u64 = 2 * 1024 * 1024;
#[cfg(test)]
std::thread_local! {
    pub(crate) static COMPLETE_BEFORE_RELEASE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(crate) const CANDIDATE_METADATA_RESERVATION: u64 =
    CANDIDATE_PROGRAM_METADATA + CANDIDATE_BINDING_METADATA;

/// Original input, bound instances and exact temporal basis for one publication attempt.
///
/// This value is not cloneable and does not borrow its Host. Retryable submission failure returns
/// the same value. Dropping it cancels unpublished work; resources already touched by a native
/// attempt independently remain with that attempt's original submitted prefix.
pub struct WgpuRenderCandidate {
    pub(crate) backend: u64,
    pub(crate) epoch: u64,
    pub(crate) producer: WgpuRenderProducerId,
    pub(crate) input: WgpuRenderInput,
    pub(crate) built: BuiltPolicy,
    pub(crate) target: Option<crate::host::ImageTargetStamp>,
    pub(crate) _reservation: CandidateReservation,
}

impl std::fmt::Debug for WgpuRenderCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuRenderCandidate")
            .field("backend", &self.backend)
            .field("epoch", &self.epoch)
            .field("producer", &self.producer)
            .field("input", &self.input)
            .finish_non_exhaustive()
    }
}

impl WgpuRenderCandidate {
    #[must_use]
    pub fn input(&self) -> &WgpuRenderInput {
        &self.input
    }
}

/// One complete policy publication, independently of whether its image is later presented.
pub struct WgpuPublishedFrame {
    pub(crate) image: crate::WgpuImageOutput,
    pub(crate) outputs: crate::WgpuProgramOutputs,
    pub(crate) history: Vec<crate::WgpuHistoryBinding>,
    pub(crate) final_version: crate::graph::ResourceVersion,
}

impl std::fmt::Debug for WgpuPublishedFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuPublishedFrame")
            .field("image", &self.image)
            .field("outputs", &self.outputs)
            .finish_non_exhaustive()
    }
}

impl WgpuPublishedFrame {
    #[must_use]
    pub fn image(&self) -> &crate::WgpuImageOutput {
        &self.image
    }

    #[must_use]
    pub fn into_image(self) -> crate::WgpuImageOutput {
        self.image
    }

    #[must_use]
    pub fn outputs(&self) -> &crate::WgpuProgramOutputs {
        &self.outputs
    }

    /// Imports the selected final image as its original typed exact program output.
    pub fn final_texture<S: 'static>(
        &self,
    ) -> Result<crate::WgpuProgramTextureOutput<S>, crate::WgpuRenderProgramError> {
        let storage = self.outputs.storage(&self.final_version)?;
        crate::program::require_descriptor::<S>(&storage.descriptor, true)?;
        Ok(crate::WgpuProgramTextureOutput::new(storage))
    }

    /// Selects this publication's exact texture, never the Host's currently latest slot.
    pub fn history_texture<S: 'static, F: crate::WgpuRenderFeature>(
        &self,
        view: nara_render::RenderViewId,
        feature: &crate::WgpuFeatureDeclaration<F>,
        slot: u64,
    ) -> Result<crate::WgpuProgramTextureOutput<S>, crate::WgpuRenderProgramError> {
        let key = crate::history::HistoryKey {
            view,
            feature: feature.id(),
            slot,
        };
        let binding = self
            .history
            .iter()
            .find(|binding| binding.key == key)
            .ok_or(crate::WgpuRenderProgramError::UnpublishedResource)?;
        let storage = self.outputs.storage(&binding.version)?;
        crate::program::require_descriptor::<S>(&storage.descriptor, true)?;
        Ok(crate::WgpuProgramTextureOutput::new(storage))
    }

    pub fn history_buffer<S: 'static, F: crate::WgpuRenderFeature>(
        &self,
        view: nara_render::RenderViewId,
        feature: &crate::WgpuFeatureDeclaration<F>,
        slot: u64,
    ) -> Result<crate::WgpuProgramBufferOutput<S>, crate::WgpuRenderProgramError> {
        let key = crate::history::HistoryKey {
            view,
            feature: feature.id(),
            slot,
        };
        let binding = self
            .history
            .iter()
            .find(|binding| binding.key == key)
            .ok_or(crate::WgpuRenderProgramError::UnpublishedResource)?;
        let storage = self.outputs.storage(&binding.version)?;
        crate::program::require_descriptor::<S>(&storage.descriptor, false)?;
        Ok(crate::WgpuProgramBufferOutput::new(storage))
    }
}

/// A failed native attempt retains its exact candidate, never a newly prepared history basis.
#[derive(Debug)]
pub struct WgpuCandidateSubmitFailure {
    pub error: WgpuRenderHostError,
    pub candidate: WgpuRenderCandidate,
}

/// Failed display preparation keeps both the pinned candidate and its original UI permissions.
#[cfg(feature = "stock-ui")]
#[derive(Debug)]
pub struct WgpuUiCandidateSubmitFailure {
    pub error: WgpuRenderHostError,
    pub candidate: WgpuRenderCandidate,
    pub ui: crate::WgpuUiSubmission,
}

impl WgpuCandidateSubmitFailure {
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        self.error.is_deferred()
    }
}

pub(crate) struct CandidateBudget {
    count: Arc<OutputBudget>,
    metadata: Arc<OutputBudget>,
}

impl Default for CandidateBudget {
    fn default() -> Self {
        Self {
            count: Arc::new(OutputBudget::new(MAX_PENDING_CANDIDATES)),
            metadata: Arc::new(OutputBudget::new(512 * 1024 * 1024)),
        }
    }
}

impl CandidateBudget {
    pub(crate) fn reserve(&self) -> Result<CandidateReservation, WgpuRenderHostError> {
        let count = self
            .count
            .reserve(1)
            .ok_or(crate::WgpuRenderProgramError::CandidateCapacity)?;
        let metadata = self
            .metadata
            .reserve(CANDIDATE_METADATA_RESERVATION)
            .ok_or(crate::WgpuRenderProgramError::CandidateCapacity)?;
        Ok(CandidateReservation {
            _count: count,
            metadata,
        })
    }

    pub(crate) fn set_limit(&self, bytes: u64) {
        self.metadata.set_limit(bytes);
    }

    pub(crate) fn retained(&self) -> u64 {
        self.metadata.retained()
    }
}

pub(crate) struct CandidateReservation {
    _count: OutputReservation,
    metadata: OutputReservation,
}

impl CandidateReservation {
    pub(crate) fn finish(&self, built: &BuiltPolicy) -> Result<(), WgpuRenderHostError> {
        let bytes = built
            .metadata_bytes
            .checked_add(built.output.program.modeled_metadata_bytes())
            .and_then(|bytes| bytes.checked_add(size_of::<WgpuRenderCandidate>()))
            .ok_or(crate::WgpuRenderProgramError::HistoryCapacity)?;
        if bytes as u64 > CANDIDATE_METADATA_RESERVATION {
            return Err(crate::WgpuRenderProgramError::HistoryCapacity.into());
        }
        // The full conservative preparation envelope remains held while the candidate is live.
        // This is bounded metadata admission, not a claim about allocator or native driver RSS.
        let _ = &self.metadata;
        Ok(())
    }
}
