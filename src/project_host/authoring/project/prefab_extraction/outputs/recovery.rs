//! Recovery data issued from the complete Review and its already fixed outputs.
//!
//! Working encodings are retained on refusal. Retrying package admission never recaptures live
//! documents, reopens files, reassigns identities or encodes completed publication outputs again.

use super::super::recovery_data::{
    PrefabExtractionRecovery, PrefabExtractionRecoveryError, PrefabExtractionRecoveryInput,
    PrefabExtractionRecoveryLimits, RecoveryBundleInput, RecoveryMetadataGuardInput,
    RecoveryNewSourceInput, RecoveryOriginalSourceInput, RecoveryOutputInput, RecoveryRootInput,
    RecoverySelectionInput, RecoverySourceDisposition, RecoverySourceEncoding,
};
use crate::project_host::authoring::source_inventory::SourceRootObservation;

use super::*;

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PreparedProjectPrefabExtractionRecovery
{
    pub(super) outputs: PreparedProjectPrefabExtractionOutputs,
    pub(super) working: Vec<Vec<u8>>,
    pub(super) encodings: Vec<SceneFileEncoding>,
    initialized: bool,
    pub(super) recovery: Option<PrefabExtractionRecovery>,
    recovery_retained_bytes: usize,
}

impl fmt::Debug for PreparedProjectPrefabExtractionRecovery {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedProjectPrefabExtractionRecovery")
            .field("outputs", &self.outputs)
            .field("working_sources", &self.working.len())
            .field("recovery", &self.recovery)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(in crate::project_host::authoring::project::prefab_extraction) enum ProjectPrefabExtractionRecoveryError
{
    Output(ProjectPrefabExtractionOutputError),
    Data(PrefabExtractionRecoveryError),
}

impl From<ProjectPrefabExtractionOutputError> for ProjectPrefabExtractionRecoveryError {
    fn from(error: ProjectPrefabExtractionOutputError) -> Self {
        Self::Output(error)
    }
}

impl From<PrefabExtractionRecoveryError> for ProjectPrefabExtractionRecoveryError {
    fn from(error: PrefabExtractionRecoveryError) -> Self {
        Self::Data(error)
    }
}

impl From<AuthoringProjectSourceAdmissionError> for ProjectPrefabExtractionRecoveryError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        Self::Output(error.into())
    }
}

#[derive(Debug)]
#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct ProjectPrefabExtractionRecoveryFailure
{
    prepared: PreparedProjectPrefabExtractionRecovery,
    error: ProjectPrefabExtractionRecoveryError,
}

impl PreparedProjectPrefabExtractionOutputs {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn prepare_recovery(
        self,
        limits: PrefabExtractionRecoveryLimits,
    ) -> Result<PreparedProjectPrefabExtractionRecovery, ProjectPrefabExtractionRecoveryFailure>
    {
        PreparedProjectPrefabExtractionRecovery {
            outputs: self,
            working: Vec::new(),
            encodings: Vec::new(),
            initialized: false,
            recovery: None,
            recovery_retained_bytes: 0,
        }
        .resume(limits)
    }
}

impl ProjectPrefabExtractionRecoveryFailure {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn error(
        &self,
    ) -> &ProjectPrefabExtractionRecoveryError {
        &self.error
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn prepared(
        &self,
    ) -> &PreparedProjectPrefabExtractionRecovery {
        &self.prepared
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn prepared_mut(
        &mut self,
    ) -> &mut PreparedProjectPrefabExtractionRecovery {
        &mut self.prepared
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn retry(
        self,
        limits: PrefabExtractionRecoveryLimits,
    ) -> Result<PreparedProjectPrefabExtractionRecovery, Self> {
        self.prepared.resume(limits)
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn cancel(
        self,
    ) -> Result<PreparedProjectPrefabExtractionOutputs, Self> {
        self.prepared.cancel()
    }
}

impl PreparedProjectPrefabExtractionRecovery {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn outputs(
        &self,
    ) -> &PreparedProjectPrefabExtractionOutputs {
        &self.outputs
    }

    /// Borrows the retained original Review for Session validation and Task accounting.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn review_mut(
        &mut self,
    ) -> &mut PreparedProjectPrefabExtractionReview {
        &mut self.outputs.review
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn recovery(
        &self,
    ) -> Option<&PrefabExtractionRecovery> {
        self.recovery.as_ref()
    }

    /// Releases package-owned Host storage only after dropping its actual data. The source
    /// allowance is cumulative, so releasing working encodings never renews extraction Work.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn cancel(
        self,
    ) -> Result<PreparedProjectPrefabExtractionOutputs, ProjectPrefabExtractionRecoveryFailure>
    {
        let Some(retained) = self
            .outputs
            .review
            .context
            .budget
            .retained_candidate_bytes
            .checked_sub(self.recovery_retained_bytes)
        else {
            return Err(ProjectPrefabExtractionRecoveryFailure {
                prepared: self,
                error: AuthoringProjectSourceAdmissionError::AccountingOverflow.into(),
            });
        };
        let Self {
            mut outputs,
            working,
            encodings,
            recovery,
            ..
        } = self;
        drop(recovery);
        drop(working);
        drop(encodings);
        // The package records its own live reservation; all other original owners remain held.
        outputs.review.context.budget.retained_candidate_bytes = retained;
        Ok(outputs)
    }

    fn resume(
        mut self,
        limits: PrefabExtractionRecoveryLimits,
    ) -> Result<Self, ProjectPrefabExtractionRecoveryFailure> {
        match self.advance(limits) {
            Ok(()) => Ok(self),
            Err(error) => Err(ProjectPrefabExtractionRecoveryFailure {
                prepared: self,
                error,
            }),
        }
    }

    fn advance(
        &mut self,
        limits: PrefabExtractionRecoveryLimits,
    ) -> Result<(), ProjectPrefabExtractionRecoveryError> {
        if !self.outputs.is_complete() {
            return Err(ProjectPrefabExtractionOutputError::Invalid(
                "Recovery preparation requires the complete fixed output owner",
            )
            .into());
        }
        if self.recovery.is_some() {
            return Ok(());
        }
        let count = self.outputs.review.context.inventory.participants().len();
        if count == 0 || count > limits.sources.get() {
            return Err(PrefabExtractionRecoveryError::SourceCountExceeded.into());
        }
        if !self.initialized {
            let bytes = checked_sum([
                size_of::<Self>(),
                size_of::<ProjectPrefabExtractionRecoveryFailure>(),
                checked_product(count, size_of::<Vec<u8>>())?,
                checked_product(count, size_of::<SceneFileEncoding>())?,
            ])
            .map_err(ProjectPrefabExtractionOutputError::from)?;
            self.outputs.admit(Some(bytes))?;
            self.working = Vec::with_capacity(count);
            self.encodings = self
                .outputs
                .review
                .context
                .inventory
                .participants()
                .map(|participant| match participant.encoding() {
                    SourceEncoding::Json => SceneFileEncoding::Json,
                    SourceEncoding::Ron => SceneFileEncoding::Ron,
                })
                .collect();
            self.initialized = true;
        }
        while self.working.len() < count {
            let index = self.working.len();
            let encoding = self.encodings[index];
            let bytes = self
                .outputs
                .review
                .persistence
                .encode_reviewed_working_source(
                    index,
                    encoding,
                    self.outputs.review.context.inventory.scene_limits(),
                )
                .map_err(ProjectPrefabExtractionOutputError::Source)?;
            self.working.push(bytes);
        }
        let before = self.outputs.review.context.budget.retained_candidate_bytes;
        let recovery = prepare_package(&mut self.outputs, &self.working, limits)?;
        self.recovery_retained_bytes =
            self.outputs.review.context.budget.retained_candidate_bytes - before;
        self.recovery = Some(recovery);
        Ok(())
    }
}

fn prepare_package(
    outputs: &mut PreparedProjectPrefabExtractionOutputs,
    working: &[Vec<u8>],
    limits: PrefabExtractionRecoveryLimits,
) -> Result<PrefabExtractionRecovery, ProjectPrefabExtractionRecoveryError> {
    let context = &outputs.review.context;
    let inventory = &context.inventory;
    let count = inventory.participants().len();
    let references = checked_sum([
        checked_product(count, size_of::<RecoveryOriginalSourceInput<'_>>())?,
        checked_product(
            count,
            size_of::<Option<EditorPrefabExtractionDisposition>>(),
        )?,
        checked_product(count, size_of::<Option<&OutputMember>>())?,
        checked_product(
            inventory.metadata().len(),
            size_of::<RecoveryMetadataGuardInput<'_>>(),
        )?,
        checked_product(inventory.roots().len(), size_of::<RecoveryRootInput<'_>>())?,
        checked_product(inventory.missing_sidecars().len(), size_of::<&str>())?,
    ])
    .map_err(ProjectPrefabExtractionOutputError::from)?;
    outputs.admit(Some(references))?;
    let before = outputs.review.context.budget.retained_candidate_bytes;
    let admission = outputs
        .review
        .context
        .budget
        .charge_source_bytes(0, 0, references, references);
    if let Err(error) = admission {
        outputs.review.context.budget.retained_candidate_bytes = before;
        return Err(ProjectPrefabExtractionOutputError::from(error).into());
    }
    let result = build_package(outputs, working, limits);
    // Every borrowed map/vector in build_package has dropped. Package admission itself refunds
    // only its failed temporaries, independently of this enclosing reference-table reservation.
    outputs
        .review
        .context
        .budget
        .reconcile_retained_reservation(references, 0)
        .map_err(ProjectPrefabExtractionOutputError::from)?;
    result
}

fn build_package(
    outputs: &mut PreparedProjectPrefabExtractionOutputs,
    working: &[Vec<u8>],
    limits: PrefabExtractionRecoveryLimits,
) -> Result<PrefabExtractionRecovery, ProjectPrefabExtractionRecoveryError> {
    let review = &mut outputs.review;
    let inventory = &review.context.inventory;
    let count = inventory.participants().len();
    let mut open = vec![None; count];
    for (binding, (document, disposition)) in review
        .persistence
        .sources()
        .prepared()
        .bindings()
        .iter()
        .zip(review.persistence.documents())
    {
        if binding.document != document || binding.source >= count || open[binding.source].is_some()
        {
            return Err(ProjectPrefabExtractionOutputError::Invalid(
                "Recovery requires every distinct original open binding",
            )
            .into());
        }
        open[binding.source] = Some(disposition);
    }
    let mut replacements = vec![None; count];
    let mut q = None;
    let mut sidecar = None;
    for member in &outputs.members {
        match member.role {
            ProjectPrefabExtractionOutputRole::Existing { source, .. } => {
                replacements[source] = Some(member);
            }
            ProjectPrefabExtractionOutputRole::ExtractedPrefab => q = Some(member),
            ProjectPrefabExtractionOutputRole::ExtractedMetadata => sidecar = Some(member),
        }
    }
    let invalid = || {
        ProjectPrefabExtractionRecoveryError::Output(ProjectPrefabExtractionOutputError::Invalid(
            "A complete fixed output is unavailable",
        ))
    };
    let q = q.ok_or_else(invalid)?;
    let sidecar = sidecar.ok_or_else(invalid)?;
    let mut sources = Vec::with_capacity(count);
    for (source, participant) in inventory.participants().enumerate() {
        let disposition = match open[source] {
            Some(EditorPrefabExtractionDisposition::Guard) => RecoverySourceDisposition::Guard,
            Some(EditorPrefabExtractionDisposition::SaveCurrent) => {
                RecoverySourceDisposition::SaveCurrent
            }
            Some(EditorPrefabExtractionDisposition::ReplaceSource) => {
                RecoverySourceDisposition::ReplaceSource
            }
            None if replacements[source].is_some() => RecoverySourceDisposition::RewriteClosed,
            None => RecoverySourceDisposition::Guard,
        };
        sources.push(RecoveryOriginalSourceInput {
            path: participant.file().path(),
            key: participant.source_key(),
            encoding: match participant.encoding() {
                SourceEncoding::Json => RecoverySourceEncoding::Json,
                SourceEncoding::Ron => RecoverySourceEncoding::Ron,
            },
            explicit_native: participant.explicit_native(),
            original_disk: participant.bytes(),
            reviewed_working: &working[source],
            disposition,
            output: match replacements[source] {
                Some(member) => Some(member_bytes(member).ok_or_else(invalid)?),
                None => None,
            },
        });
    }
    let metadata = inventory
        .metadata()
        .map(|actual| RecoveryMetadataGuardInput {
            path: actual.file().path(),
            observed: actual.bytes(),
        })
        .collect::<Vec<_>>();
    let roots = inventory
        .roots()
        .map(|actual| RecoveryRootInput {
            path: actual.path(),
            missing_ancestor: match actual.observation() {
                SourceRootObservation::Available { .. } => None,
                SourceRootObservation::Missing { ancestor_path, .. } => Some(ancestor_path),
            },
        })
        .collect::<Vec<_>>();
    let missing = inventory.missing_sidecars().collect::<Vec<_>>();
    let schema = &review.context.schema;
    let corpus = review.persistence.sources().prepared().corpus();
    PrefabExtractionRecovery::create(
        PrefabExtractionRecoveryInput {
            old_ready_manifest: schema.manifest_bytes(),
            final_ready_manifest: schema.manifest_bytes(),
            unchanged_bundle: RecoveryBundleInput {
                path: schema.bundle_path().as_str(),
                blob: schema.bundle_bytes(),
            },
            source_universe: schema.settings().project.source_universe,
            selected_profile: schema.settings().profile_name.as_deref(),
            selection: RecoverySelectionInput {
                source: corpus.selected_source(),
                root: corpus.root(),
            },
            complete_original_sources: &sources,
            q: RecoveryNewSourceInput {
                path: &q.path,
                key: &review.context.destination.key,
                encoding: match review.context.destination.encoding {
                    SceneFileEncoding::Json => RecoverySourceEncoding::Json,
                    SceneFileEncoding::Ron => RecoverySourceEncoding::Ron,
                },
                output: member_bytes(q).ok_or_else(invalid)?,
            },
            q_sidecar: RecoveryOutputInput {
                path: &sidecar.path,
                output: member_bytes(sidecar).ok_or_else(invalid)?,
            },
            metadata_guards: &metadata,
            roots: &roots,
            missing_sidecars: &missing,
        },
        limits,
        &mut review.context.budget,
    )
    .map_err(Into::into)
}

fn member_bytes(member: &OutputMember) -> Option<&[u8]> {
    member.payload.as_ref().and_then(Payload::bytes)
}
