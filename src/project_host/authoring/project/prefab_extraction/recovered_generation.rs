//! Stored extraction semantics under one actual Pending observation and unchanged basis.
//!
//! Recovery reads fixed identities and declarations. It does not rerun extraction, recreate a
//! Workspace, publish sources, or turn observed output bytes into a replacement receipt.

use nara_scene::__private::{
    PrefabExtractionSavedGenerationError, PrefabExtractionUsage, PreparedPrefabExtraction,
};

use super::{
    recovered_inventory::{
        RecoveredPrefabExtractionInventory, RecoveredPrefabExtractionInventoryError,
    },
    recovery_data::{PrefabExtractionRecovery, PrefabExtractionRecoveryError},
    *,
};
use crate::project_host::authoring::{
    AuthoringProjectAdmissionUsage, recovery_storage::ObservedPendingRecoveryStorage,
    source_inventory::SourceInventoryError,
};

mod capture;

/// Decoded inputs remain owned on a semantic refusal. Retry spends the original allowances and
/// reuses these exact allocations, including Q's chosen entity IDs and input names.
struct StoredGeneration {
    disk: Vec<CapturedExtractionSource>,
    request: PrefabExtractionRequest,
    outputs: Vec<CapturedExtractionSource>,
    extracted: ResolvedPrefabSource,
    published: Vec<bool>,
    work: PrefabExtractionWork,
}

enum Stage {
    Decode(PrefabExtractionWork),
    Verify(StoredGeneration),
    Saved {
        corpus: PreparedPrefabExtraction,
        disk: Vec<CapturedExtractionSource>,
        published: Vec<bool>,
    },
    Complete {
        corpus: PreparedPrefabExtraction,
        generation: PrefabExtractionSavedGeneration,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveredPrefabExtractionGenerationPhase {
    Decode,
    Verify,
    Saved,
    Complete,
}

/// Physical observation, semantic admission and their continuing allowances move together.
/// Callers cannot detach the proof from the native corpus which supplied its actual basis.
#[must_use]
pub(super) struct RecoveredPrefabExtractionGeneration {
    pending: ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    observation: RecoveredPrefabExtractionInventory,
    scene_limits: SceneFileLimits,
    budget: AuthoringProjectAdmissionBudget,
    stage: Stage,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum RecoveredPrefabExtractionGenerationError {
    #[error("the actual Pending extraction inventory changed")]
    Inventory(#[from] RecoveredPrefabExtractionInventoryError),
    #[error("stored extraction source admission failed")]
    Source(#[from] PrefabExtractionProjectError),
    #[error("stored extraction source decoding failed")]
    Decode(#[from] SourceDecodeError),
    #[error("stored extraction destination metadata decoding failed")]
    Data(#[from] PrefabExtractionRecoveryError),
    #[error("stored extraction metadata differs from its admitted destination")]
    Destination,
    #[error("stored extraction admission exceeded its original source allowance")]
    SourceWork(ScenePatchReport),
    #[error("stored extraction outputs do not preserve the complete reviewed source relation")]
    Correspondence {
        report: ScenePatchReport,
        environment: Option<usize>,
    },
    #[error("stored extraction outputs and original disk guards are not a valid saved generation")]
    SavedGeneration(PrefabExtractionSavedGenerationError),
}

impl From<PrefabExtractionSavedGenerationError> for RecoveredPrefabExtractionGenerationError {
    fn from(error: PrefabExtractionSavedGenerationError) -> Self {
        Self::SavedGeneration(error)
    }
}

impl From<SourceInventoryError> for RecoveredPrefabExtractionGenerationError {
    fn from(error: SourceInventoryError) -> Self {
        Self::Inventory(error.into())
    }
}

impl From<AuthoringProjectAdmissionError> for RecoveredPrefabExtractionGenerationError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self::Inventory(error.into())
    }
}

impl From<AuthoringProjectSourceAdmissionError> for RecoveredPrefabExtractionGenerationError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        Self::Source(error.into())
    }
}

#[derive(Debug)]
#[must_use]
pub(super) struct RecoveredPrefabExtractionGenerationFailure {
    owner: RecoveredPrefabExtractionGeneration,
    error: RecoveredPrefabExtractionGenerationError,
}

impl fmt::Debug for RecoveredPrefabExtractionGeneration {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveredPrefabExtractionGeneration")
            .field("operation", &self.pending.recovery().digest())
            .field("phase", &self.phase())
            .field("usage", &self.source_usage())
            .finish_non_exhaustive()
    }
}

impl RecoveredPrefabExtractionGenerationFailure {
    pub(super) fn into_parts(
        self,
    ) -> (
        RecoveredPrefabExtractionGeneration,
        RecoveredPrefabExtractionGenerationError,
    ) {
        (self.owner, self.error)
    }

    pub(super) fn error(&self) -> &RecoveredPrefabExtractionGenerationError {
        &self.error
    }

    pub(super) fn owner(&self) -> &RecoveredPrefabExtractionGeneration {
        &self.owner
    }

    pub(super) fn retry(self) -> Result<RecoveredPrefabExtractionGeneration, Self> {
        self.owner.advance()
    }
}

impl RecoveredPrefabExtractionGeneration {
    pub(super) fn publication_inputs(
        &mut self,
    ) -> Option<super::publication::PublicationInputs<'_>> {
        if !matches!(self.stage, Stage::Complete { .. }) {
            return None;
        }
        Some(super::publication::PublicationInputs {
            pending: &self.pending,
            inventory: self.observation.inventory(),
            budget: &mut self.budget,
            #[cfg(feature = "tooling")]
            original_open: None,
            bundle: super::publication::PublicationBundle {
                path: self.pending.recovery().unchanged_bundle().path(),
                identity: self.observation.bundle_file().identity(),
                parent: self.observation.bundle_file().parent_identity(),
                digest: self.observation.bundle_digest(),
            },
            schema: super::publication::PublicationSchema::Recovered(self.observation.models()),
        })
    }

    /// The caller transfers its actual observation and the allowances used to load/capture it.
    /// The first step revalidates their binding before decoding any stored source.
    pub(super) fn new(
        pending: ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
        observation: RecoveredPrefabExtractionInventory,
        budget: AuthoringProjectAdmissionBudget,
        work: PrefabExtractionWork,
    ) -> Self {
        let scene_limits = observation.inventory().scene_limits();
        Self {
            pending,
            observation,
            scene_limits,
            budget,
            stage: Stage::Decode(work),
        }
    }

    pub(super) fn phase(&self) -> RecoveredPrefabExtractionGenerationPhase {
        match self.stage {
            Stage::Decode(_) => RecoveredPrefabExtractionGenerationPhase::Decode,
            Stage::Verify(_) => RecoveredPrefabExtractionGenerationPhase::Verify,
            Stage::Saved { .. } => RecoveredPrefabExtractionGenerationPhase::Saved,
            Stage::Complete { .. } => RecoveredPrefabExtractionGenerationPhase::Complete,
        }
    }

    pub(super) fn pending(&self) -> &ObservedPendingRecoveryStorage<PrefabExtractionRecovery> {
        &self.pending
    }

    pub(super) fn observation(&self) -> &RecoveredPrefabExtractionInventory {
        &self.observation
    }

    pub(super) fn host_usage(&self) -> AuthoringProjectAdmissionUsage {
        self.budget.usage()
    }

    pub(super) fn source_usage(&self) -> PrefabExtractionUsage {
        match &self.stage {
            Stage::Decode(work) => work.usage(),
            Stage::Verify(stored) => stored.work.usage(),
            Stage::Saved { corpus, .. } | Stage::Complete { corpus, .. } => corpus.usage(),
        }
    }

    pub(super) fn verified(
        &self,
    ) -> Option<(&PreparedPrefabExtraction, &PrefabExtractionSavedGeneration)> {
        match &self.stage {
            Stage::Complete {
                corpus, generation, ..
            } => Some((corpus, generation)),
            _ => None,
        }
    }

    /// Rechecking does not capture another corpus, admit another basis or renew either budget.
    pub(super) fn validate(&mut self) -> Result<(), RecoveredPrefabExtractionGenerationError> {
        self.observation.validate(&self.pending, &mut self.budget)?;
        if let Stage::Complete {
            corpus, generation, ..
        } = &mut self.stage
        {
            corpus
                .require_saved_generation(generation)
                .map_err(RecoveredPrefabExtractionGenerationError::SourceWork)?;
        }
        Ok(())
    }

    /// Each step either advances one finite stage or returns the same complete owner.
    pub(super) fn advance(mut self) -> Result<Self, RecoveredPrefabExtractionGenerationFailure> {
        if let Err(error) = self.validate() {
            return Err(RecoveredPrefabExtractionGenerationFailure { owner: self, error });
        }
        let Self {
            pending,
            observation,
            scene_limits,
            mut budget,
            stage,
        } = self;
        let (stage, refusal) = match stage {
            Stage::Decode(mut work) => {
                match capture::sources(&pending, &observation, scene_limits, &mut budget, &mut work)
                {
                    Ok(inputs) => (
                        Stage::Verify(StoredGeneration {
                            disk: inputs.disk,
                            request: inputs.request,
                            outputs: inputs.outputs,
                            extracted: inputs.extracted,
                            published: inputs.published,
                            work,
                        }),
                        None,
                    ),
                    Err(error) => (Stage::Decode(work), Some(error)),
                }
            }
            Stage::Verify(stored) => {
                let StoredGeneration {
                    disk,
                    request,
                    outputs,
                    extracted,
                    published,
                    work,
                } = stored;
                match request.admit_outputs(outputs, extracted, work) {
                    Ok(corpus) => (
                        Stage::Saved {
                            corpus,
                            disk,
                            published,
                        },
                        None,
                    ),
                    Err(failure) => {
                        let environment = failure.environment_index();
                        let (request, outputs, extracted, work, report) = failure.into_parts();
                        (
                            Stage::Verify(StoredGeneration {
                                disk,
                                request,
                                outputs,
                                extracted,
                                published,
                                work,
                            }),
                            Some(RecoveredPrefabExtractionGenerationError::Correspondence {
                                report,
                                environment,
                            }),
                        )
                    }
                }
            }
            Stage::Saved {
                mut corpus,
                disk,
                published,
            } => match corpus.prepare_saved_generation(&disk, &published) {
                Ok(generation) => (Stage::Complete { corpus, generation }, None),
                Err(error) => (
                    Stage::Saved {
                        corpus,
                        disk,
                        published,
                    },
                    Some(error.into()),
                ),
            },
            complete @ Stage::Complete { .. } => (complete, None),
        };
        let owner = Self {
            pending,
            observation,
            scene_limits,
            budget,
            stage,
        };
        match refusal {
            Some(error) => Err(RecoveredPrefabExtractionGenerationFailure { owner, error }),
            None => Ok(owner),
        }
    }
}
