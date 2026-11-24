//! Explicit continuation of an authenticated, fixed component Apply generation.

use std::{fmt, mem::size_of, sync::Arc};

use nara_fs::{DirectoryCapability, ReplaceReceipt, StageStatus};
use nara_project::ProjectAuthoringRecoveryDigest;
use nara_scene::{SceneFileEncoding, SceneFileLimits, SourceDocumentKey};

use super::{
    ComponentApplyReplacement, ComponentApplySavedGenerationError, ComponentApplySourceBody,
    physical::{
        ComponentApplyPhysicalError, ComponentApplyPhysicalInventory, capture_pending_inventory,
    },
    publication::{ComponentApplyPublication, ComponentApplyPublicationError},
    recovery_data::{
        PrefabComponentApplyRecoveryData, PrefabComponentApplyRecoveryError as PackageError,
        PrefabComponentApplyRecoveryLimits, RecoveryBlob, RecoverySource,
    },
    validate_component_apply_saved_generation,
};
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
        AuthoringProjectAdmissionUsage, AuthoringSourceOperationLimits,
        pending_recovery::{
            PendingAuthoringRecoveryInspection, PendingAuthoringRecoveryInspectionError,
            PendingAuthoringRecoveryInspectionErrorKind, PendingAuthoringRecoveryKind,
            PendingAuthoringRecoveryUsage,
        },
        recovery_storage::{ObservedPendingRecoveryStorage, RecoveryStorageError},
        schema_models::{SchemaModelsError, prepare_schema_models},
        source_decode::{SourceDecodeError, SourceTree, decode_source},
        source_inventory::{SourceEncoding, SourceInventoryError, with_temporary_storage},
    },
    file_publication::{FilePublicationEvidence, FilePublicationRejection},
};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

type Pending = ObservedPendingRecoveryStorage<PrefabComponentApplyRecoveryData>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabComponentApplyRecoveryPhase {
    Review,
    PublishingSources,
    PublishingReady,
    Complete,
}

#[derive(Debug, Clone, Copy)]
pub struct PrefabComponentApplyRecoveryView {
    pub operation: ProjectAuthoringRecoveryDigest,
    pub phase: PrefabComponentApplyRecoveryPhase,
    pub source_count: usize,
    /// Number of the two fixed outputs accepted in this process, including observations.
    pub completed_files: usize,
    pub refusal: Option<PrefabComponentApplyRecoveryErrorKind>,
    pub cleanup_pending: bool,
    pub uncertain: bool,
    pub limits: AuthoringSourceOperationLimits,
    pub usage: PendingAuthoringRecoveryUsage,
}

#[derive(Debug, Clone, Copy)]
pub struct PrefabComponentApplyRecoveryOutput<'a> {
    pub key: &'a SourceDocumentKey,
    pub project_path: &'a str,
    pub encoding: SceneFileEncoding,
    pub original_disk: &'a [u8],
    pub output: &'a [u8],
    /// The actual leaf already contains the fixed output, including byte-identical pairs.
    pub already_current: bool,
}

pub struct PrefabComponentApplyRecoveryReview<'a> {
    capture: &'a Captured,
}

impl<'a> PrefabComponentApplyRecoveryReview<'a> {
    pub fn selected_profile(&self) -> Option<&'a str> {
        self.capture.pending.recovery().selected_profile()
    }

    /// Containing source first, then the existing destination Prefab.
    pub fn outputs(&self) -> [PrefabComponentApplyRecoveryOutput<'a>; 2] {
        let outputs = self.capture.pending.recovery().outputs();
        let current = self.capture.physical.output_is_current();
        std::array::from_fn(|index| {
            let (source, output) = outputs[index];
            PrefabComponentApplyRecoveryOutput {
                key: source.key(),
                project_path: source.path(),
                encoding: scene_encoding(source.encoding()),
                original_disk: source.original_disk().bytes(),
                output: output.bytes(),
                already_current: current[index],
            }
        })
    }
}

struct Captured {
    pending: Pending,
    physical: ComponentApplyPhysicalInventory,
    budget: AuthoringProjectAdmissionBudget,
}

enum Stage {
    Review(Captured),
    Publishing(ComponentApplyPublication),
    Complete(PrefabComponentApplyRecoveryReceipt),
}

/// The headless owner of one Pending operation, from read-only review through accepted Ready.
/// Confirmation binds the exact captured leaves. Each subsequent advance accepts one output
/// or publishes Ready. Refusal retains the publisher and its original cumulative allowance.
#[must_use]
pub struct PrefabComponentApplyRecovery {
    stage: Option<Stage>,
    operation: ProjectAuthoringRecoveryDigest,
    source_count: usize,
    limits: AuthoringSourceOperationLimits,
    failure: Option<PrefabComponentApplyRecoveryError>,
}

impl fmt::Debug for PrefabComponentApplyRecovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabComponentApplyRecovery")
            .field("view", &self.view())
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl PrefabComponentApplyRecovery {
    pub fn inspect(
        root: DirectoryCapability,
        limits: AuthoringSourceOperationLimits,
    ) -> Result<Self, PrefabComponentApplyRecoveryInspectionFailure> {
        let inspection =
            PendingAuthoringRecoveryInspection::inspect(root, limits).map_err(|error| {
                PrefabComponentApplyRecoveryInspectionFailure::new(None, Failure::Inspect(error))
            })?;
        Self::from_inspection(inspection)
    }

    /// Consumes the original authenticated read and allowance. A different known kind returns
    /// the original inspection so it can be dispatched to its own recovery operation.
    pub fn from_inspection(
        inspection: PendingAuthoringRecoveryInspection,
    ) -> Result<Self, PrefabComponentApplyRecoveryInspectionFailure> {
        if inspection.kind() != PendingAuthoringRecoveryKind::PrefabComponentApply {
            return Err(PrefabComponentApplyRecoveryInspectionFailure::new(
                Some(inspection),
                Failure::WrongKind,
            ));
        }
        Self::admit_inspected(inspection)
            .map_err(|error| PrefabComponentApplyRecoveryInspectionFailure::new(None, error))
    }

    fn admit_inspected(inspection: PendingAuthoringRecoveryInspection) -> Result<Self, Failure> {
        let (loaded, mut budget, limits) = inspection.into_parts();
        budget.charge_source_bytes(0, 0, 0, size_of::<Self>())?;
        let file_limits = PrefabComponentApplyRecoveryLimits::default();
        let recovery = PrefabComponentApplyRecoveryData::decode(
            loaded.encoded_bytes(),
            file_limits,
            &mut budget,
        )?;
        let pending = loaded
            .bind(recovery, &mut budget)
            .map_err(|failure| failure.error)?;
        let physical = capture_pending_inventory(&pending, file_limits.scene, &mut budget)?;
        validate_fixed_generation(&pending, &physical, file_limits.scene, &mut budget)?;
        physical.validate_authority(&pending, &mut budget)?;
        Ok(Self {
            operation: pending.reference().recovery_digest(),
            source_count: pending.recovery().complete_original_sources().len(),
            limits,
            failure: None,
            stage: Some(Stage::Review(Captured {
                pending,
                physical,
                budget,
            })),
        })
    }

    pub fn review(&self) -> Option<PrefabComponentApplyRecoveryReview<'_>> {
        match self.stage() {
            Stage::Review(capture) => Some(PrefabComponentApplyRecoveryReview { capture }),
            _ => None,
        }
    }

    pub fn view(&self) -> PrefabComponentApplyRecoveryView {
        use PrefabComponentApplyRecoveryPhase as Phase;
        let (phase, completed_files, cleanup_pending, usage) = match self.stage() {
            Stage::Review(capture) => (Phase::Review, 0, false, host_usage(capture.budget.usage())),
            Stage::Publishing(publication) => {
                let completed = (0..2)
                    .filter(|&index| publication.accepted_output(index).is_some())
                    .count();
                let phase = if completed == 2 {
                    Phase::PublishingReady
                } else {
                    Phase::PublishingSources
                };
                (
                    phase,
                    completed,
                    publication.cleanup_pending(),
                    host_usage(publication.usage()),
                )
            }
            Stage::Complete(receipt) => (Phase::Complete, 2, false, receipt.usage),
        };
        PrefabComponentApplyRecoveryView {
            operation: self.operation,
            phase,
            source_count: self.source_count,
            completed_files,
            refusal: self.failure().map(PrefabComponentApplyRecoveryError::kind),
            cleanup_pending,
            uncertain: self.uncertain(),
            limits: self.limits,
            usage,
        }
    }

    pub fn failure(&self) -> Option<&PrefabComponentApplyRecoveryError> {
        self.failure.as_ref()
    }

    pub fn uncertain_publication(&self) -> Option<&ReplaceReceipt> {
        match self.stage() {
            Stage::Publishing(publication) => publication
                .uncertain_evidence()
                .map(FilePublicationEvidence::receipt),
            _ => None,
        }
    }

    pub fn confirm(
        &mut self,
    ) -> Result<PrefabComponentApplyRecoveryView, &PrefabComponentApplyRecoveryError> {
        if self.failure.is_some() {
            return self.result();
        }
        let Some(Stage::Review(capture)) = self.stage.as_mut() else {
            return Err(&WRONG_PHASE);
        };
        if let Err(error) = capture
            .physical
            .validate(&capture.pending, &mut capture.budget)
        {
            self.failure = Some(Failure::Physical(error).into());
            return self.result();
        }
        let Some(Stage::Review(capture)) = self.stage.take() else {
            unreachable!("a confirmed review retains its captured authority")
        };
        self.stage = Some(Stage::Publishing(ComponentApplyPublication::new(
            capture.pending,
            capture.physical,
            capture.budget,
        )));
        Ok(self.view())
    }

    pub fn advance(
        &mut self,
    ) -> Result<PrefabComponentApplyRecoveryView, &PrefabComponentApplyRecoveryError> {
        if self.failure.is_some() {
            return self.result();
        }
        match self.stage.as_mut().expect("one retained recovery stage") {
            Stage::Review(_) => return Err(&WRONG_PHASE),
            Stage::Publishing(publication) if publication.accepted_output(1).is_none() => {
                if let Err(error) = publication.advance() {
                    self.failure = Some(Failure::Publication(error).into());
                }
            }
            Stage::Publishing(publication) => match publication.publish_ready(|input, _| {
                let written = input
                    .outputs
                    .iter()
                    .filter(|output| output.evidence().is_some())
                    .count();
                Ok::<_, Failure>(written)
            }) {
                Ok((written_sources, ready)) => {
                    let (publication_evidence, _) = ready.into_parts();
                    self.stage = Some(Stage::Complete(PrefabComponentApplyRecoveryReceipt {
                        operation: self.operation,
                        written_sources,
                        usage: host_usage(publication.usage()),
                        publication: publication_evidence,
                    }));
                }
                Err(error) => self.failure = Some(error.into()),
            },
            Stage::Complete(_) => {}
        }
        self.result()
    }

    /// Can clean the current temporary file without advancing publication.
    pub fn retry_cleanup(&mut self) -> Result<bool, &PrefabComponentApplyRecoveryError> {
        let Some(Stage::Publishing(publication)) = self.stage.as_mut() else {
            return Ok(true);
        };
        match publication.retry_cleanup() {
            Ok(cleaned) => Ok(cleaned),
            Err(error) => Err(self.failure.insert(Failure::Publication(error).into())),
        }
    }

    /// Retries the refused boundary under the same retained authority and allowance.
    pub fn retry(
        &mut self,
    ) -> Result<PrefabComponentApplyRecoveryView, &PrefabComponentApplyRecoveryError> {
        if self.failure.is_none() || self.uncertain() {
            return self.result();
        }
        if !matches!(self.retry_cleanup(), Ok(true)) {
            return self.result();
        }
        self.failure = None;
        match self.stage() {
            Stage::Review(_) => self.confirm(),
            _ => self.advance(),
        }
    }

    /// Returns this process's actual Ready publication evidence.
    #[allow(clippy::result_large_err)]
    pub fn into_receipt(mut self) -> Result<PrefabComponentApplyRecoveryReceipt, Self> {
        if !matches!(self.stage(), Stage::Complete(_)) {
            return Err(self);
        }
        let Some(Stage::Complete(receipt)) = self.stage.take() else {
            unreachable!("completion retains an accepted Ready receipt")
        };
        Ok(receipt)
    }

    fn stage(&self) -> &Stage {
        self.stage.as_ref().expect("one retained recovery stage")
    }

    fn uncertain(&self) -> bool {
        self.failure().is_some_and(|error| {
            error.kind() == PrefabComponentApplyRecoveryErrorKind::PublicationUncertain
        }) || matches!(self.stage(), Stage::Publishing(publication) if publication.uncertain())
    }

    fn result(
        &self,
    ) -> Result<PrefabComponentApplyRecoveryView, &PrefabComponentApplyRecoveryError> {
        match self.failure() {
            Some(error) => Err(error),
            None => Ok(self.view()),
        }
    }
}

fn validate_fixed_generation(
    pending: &Pending,
    physical: &ComponentApplyPhysicalInventory,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), Failure> {
    with_temporary_storage(budget, |budget| {
        let recovery = pending.recovery();
        let models = prepare_schema_models(recovery.unchanged_bundle().blob().bytes(), budget)?;
        budget.charge_source_bytes(0, 0, 0, 4 * size_of::<usize>())?;
        let [containing, destination] = recovery.outputs();
        let replacements = [
            decode_output(containing, limits, budget)?,
            decode_output(destination, limits, budget)?,
        ];
        validate_component_apply_saved_generation(
            pending.root(),
            physical.inventory(),
            &models.basis,
            &replacements,
            budget,
        )?;
        Ok(())
    })
}

fn decode_output<'a>(
    (source, output): (&'a RecoverySource, &RecoveryBlob),
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ComponentApplyReplacement<'a>, SourceDecodeError> {
    let body = match decode_source(
        source.key().kind(),
        scene_encoding(source.encoding()),
        output.bytes(),
        limits,
        budget,
    )? {
        SourceTree::Scene(document) => ComponentApplySourceBody::Scene(Arc::new(document)),
        SourceTree::Prefab(document) => ComponentApplySourceBody::Prefab(Arc::new(document)),
    };
    Ok(ComponentApplyReplacement {
        path: source.path(),
        key: source.key(),
        body,
    })
}

fn scene_encoding(encoding: SourceEncoding) -> SceneFileEncoding {
    match encoding {
        SourceEncoding::Json => SceneFileEncoding::Json,
        SourceEncoding::Ron => SceneFileEncoding::Ron,
    }
}

fn host_usage(usage: AuthoringProjectAdmissionUsage) -> PendingAuthoringRecoveryUsage {
    PendingAuthoringRecoveryUsage {
        host_work: usage.decode_work(),
        peak_open_handles: usage.peak_open_handles(),
    }
}

#[derive(Debug)]
pub struct PrefabComponentApplyRecoveryReceipt {
    operation: ProjectAuthoringRecoveryDigest,
    written_sources: usize,
    usage: PendingAuthoringRecoveryUsage,
    publication: FilePublicationEvidence,
}

impl PrefabComponentApplyRecoveryReceipt {
    pub const fn operation(&self) -> ProjectAuthoringRecoveryDigest {
        self.operation
    }

    pub const fn written_sources(&self) -> usize {
        self.written_sources
    }

    pub const fn usage(&self) -> PendingAuthoringRecoveryUsage {
        self.usage
    }

    pub const fn ready_publication(&self) -> &ReplaceReceipt {
        self.publication.receipt()
    }

    pub const fn parent_directory_sync(&self) -> StageStatus {
        self.publication.parent_directory_sync()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct PrefabComponentApplyRecoveryInspectionFailure {
    inspection: Option<PendingAuthoringRecoveryInspection>,
    #[source]
    error: PrefabComponentApplyRecoveryError,
}

impl PrefabComponentApplyRecoveryInspectionFailure {
    fn new(inspection: Option<PendingAuthoringRecoveryInspection>, error: Failure) -> Self {
        Self {
            inspection,
            error: error.into(),
        }
    }

    pub fn error(&self) -> &PrefabComponentApplyRecoveryError {
        &self.error
    }

    pub fn into_parts(
        self,
    ) -> (
        Option<PendingAuthoringRecoveryInspection>,
        PrefabComponentApplyRecoveryError,
    ) {
        (self.inspection, self.error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabComponentApplyRecoveryErrorKind {
    Phase,
    WrongKind,
    Pending,
    Admission,
    Package,
    Inventory,
    Schema,
    SourceDecoding,
    SavedGeneration,
    PublicationRejected,
    PublicationFailed,
    PublicationUncertain,
    CleanupPending,
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct PrefabComponentApplyRecoveryError(#[from] Failure);

static WRONG_PHASE: PrefabComponentApplyRecoveryError =
    PrefabComponentApplyRecoveryError(Failure::Phase);

impl PrefabComponentApplyRecoveryError {
    pub fn kind(&self) -> PrefabComponentApplyRecoveryErrorKind {
        use {
            ComponentApplyPublicationError as Publication,
            PrefabComponentApplyRecoveryErrorKind as Kind,
        };
        match &self.0 {
            Failure::Phase | Failure::Publication(Publication::Phase) => Kind::Phase,
            Failure::WrongKind => Kind::WrongKind,
            Failure::Inspect(error)
                if error.kind() == PendingAuthoringRecoveryInspectionErrorKind::Admission =>
            {
                Kind::Admission
            }
            Failure::Inspect(_) | Failure::Storage(_) => Kind::Pending,
            Failure::Admission(_) | Failure::Publication(Publication::Admission(_)) => {
                Kind::Admission
            }
            Failure::Package(_) => Kind::Package,
            Failure::Physical(_)
            | Failure::Inventory(_)
            | Failure::Publication(Publication::Physical(_) | Publication::Inventory(_)) => {
                Kind::Inventory
            }
            Failure::Schema(_) => Kind::Schema,
            Failure::Decode(_) => Kind::SourceDecoding,
            Failure::SavedGeneration(_) => Kind::SavedGeneration,
            Failure::Publication(Publication::Failed(_)) => Kind::PublicationFailed,
            Failure::Publication(
                Publication::Rejected(FilePublicationRejection::RequiresReconcile)
                | Publication::Uncertain,
            ) => Kind::PublicationUncertain,
            Failure::Publication(Publication::Rejected(_)) => Kind::PublicationRejected,
            Failure::Publication(Publication::CleanupPending) => Kind::CleanupPending,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("the component Apply recovery action does not match its current phase")]
    Phase,
    #[error("the authenticated Pending package belongs to another recovery operation")]
    WrongKind,
    #[error(transparent)]
    Inspect(#[from] PendingAuthoringRecoveryInspectionError),
    #[error(transparent)]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error(transparent)]
    Storage(#[from] RecoveryStorageError),
    #[error(transparent)]
    Physical(#[from] ComponentApplyPhysicalError),
    #[error(transparent)]
    Inventory(#[from] SourceInventoryError),
    #[error(transparent)]
    Schema(#[from] SchemaModelsError),
    #[error(transparent)]
    Decode(#[from] SourceDecodeError),
    #[error(transparent)]
    SavedGeneration(#[from] ComponentApplySavedGenerationError),
    #[error(transparent)]
    Publication(#[from] ComponentApplyPublicationError),
}
