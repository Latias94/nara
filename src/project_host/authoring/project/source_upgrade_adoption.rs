//! Saved project preparation while the upgrade's actual source set is still Pending.
//!
//! This joins the existing saved-startup compiler to one actual upgrade Schema admission. It
//! does not replace complete all-profile membership, working-history or persistence-lane guards.
//! The enclosing operation binds those guards before invoking this owner's final publisher.

use nara_scene::AuthoringSchemaBasis;

use super::super::source_upgrade_ready::{
    BoundSourceUpgradeReady, PreparedSourceUpgradeReady, PublishedSourceUpgradeReady,
    SourceUpgradeReadyError, SourceUpgradeReadyOutcome,
};
use super::super::{
    source_upgrade_inventory::{
        PreparedSourceUpgradeInventoryDisposal, SourceUpgradeInventoryError,
        VerifiedSourceUpgradeInventory,
    },
    source_upgrade_recovery::SourceUpgradeRecovery,
};
use super::*;
use crate::project_host::file_publication::{
    FilePublicationEvidence, FilePublicationFailureStage, FilePublicationRejection,
};

/// All parsing, Adapter storage, source classification and startup closure preparation completed
/// before Ready. No ordinary snapshot reader can access this unpublished Session.
pub(crate) struct PreparedSourceUpgradeProject {
    session: AuthoringProjectSessionSnapshot,
    basis: AuthoringSchemaBasis,
    verified: VerifiedSourceUpgradeInventory,
    recovery: Arc<SourceUpgradeRecovery>,
    disposal: PreparedSourceUpgradeInventoryDisposal,
    retained_session_bytes: usize,
    retained_session_handles: usize,
}

impl PreparedSourceUpgradeProject {
    pub(crate) fn prepare(
        ready: &PreparedSourceUpgradeReady,
        verified: VerifiedSourceUpgradeInventory,
        recovery: Arc<SourceUpgradeRecovery>,
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeProjectPreparationFailure> {
        let retained_before = budget.retained_candidate_bytes;
        let handles_before = budget.retained_open_handles;
        let result = Self::prepare_session(ready, &verified, &recovery, scene_limits, budget);
        match result {
            Ok((session, basis, disposal, retained_session_bytes, retained_session_handles)) => {
                Ok(Self {
                    session,
                    basis,
                    verified,
                    recovery,
                    disposal,
                    retained_session_bytes,
                    retained_session_handles,
                })
            }
            Err(error) => {
                // The borrowed inputs still own their original reservations. All locals from
                // this attempt have dropped, including a privately constructed Session.
                let released = budget
                    .retained_candidate_bytes
                    .checked_sub(retained_before)
                    .expect("project preparation releases only its own candidate storage");
                budget
                    .reconcile_retained_reservation(released, 0)
                    .expect("the failed project candidate's reservation remains charged");
                let closed = budget
                    .retained_open_handles
                    .checked_sub(handles_before)
                    .expect("project preparation keeps its borrowed input handles alive");
                budget.release_open_handles(closed);
                Err(SourceUpgradeProjectPreparationFailure {
                    verified,
                    recovery,
                    error,
                })
            }
        }
    }

    fn prepare_session(
        ready: &PreparedSourceUpgradeReady,
        verified: &VerifiedSourceUpgradeInventory,
        recovery: &SourceUpgradeRecovery,
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<
        (
            AuthoringProjectSessionSnapshot,
            AuthoringSchemaBasis,
            PreparedSourceUpgradeInventoryDisposal,
            usize,
            usize,
        ),
        SourceUpgradeProjectError,
    > {
        let session_bytes_before = budget.retained_candidate_bytes;
        let session_handles_before = budget.retained_open_handles;
        ready.validate(budget)?;
        let schema = ready.observed_pending_schema();
        verified.validate_retained(schema, recovery, budget)?;
        let basis = ready.basis().clone();
        let adapter_bytes = adapter_receipts_retained_bytes(ready.adapters())?;
        budget.charge_source_bytes(
            0,
            0,
            adapter_bytes,
            checked_sum([
                adapter_bytes,
                size_of::<Self>(),
                size_of::<AuthoringProjectSnapshotInner>(),
                size_of::<AuthoringProjectSessionSnapshot>(),
            ])?,
        )?;
        let adapters = ready.adapters().to_vec().into_boxed_slice();
        let read = read_saved_startup(schema, scene_limits, budget)?;
        let mut startup = capture_saved_startup(
            schema,
            scene_limits,
            read,
            &basis,
            schema.bundle_bytes().len(),
            budget,
            &[],
            &[],
            capture::AuthoringCapturePreview::Prepare,
        )?;
        #[cfg(feature = "presentation-2d")]
        reserve_preview_identity_store(&startup.capture, budget)?;
        let index = ProjectSourceIndexState::checked_retained_bytes_for_schema(schema)?;
        budget.charge_source_bytes(0, 0, index, index)?;
        startup.capture.usage = budget.usage();
        let candidate =
            startup.into_candidate(Arc::clone(schema), adapters, scene_limits, budget.usage());
        // This ordinary receipt conversion allocates index/storage and can refuse aliasing;
        // it therefore belongs here, before the final Ready transition.
        let session = candidate
            .publish()
            .into_session_snapshot()
            .map_err(|_| SourceUpgradeProjectError::PrivateStorage)?;
        // Determine this successor's continuing scope before Ready, independently of the
        // original inventory, proposal attempts, recovery and final binding collections.
        let session_bytes = budget
            .retained_candidate_bytes
            .checked_sub(session_bytes_before)
            .and_then(|bytes| bytes.checked_sub(size_of::<Self>()))
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let session_handles = budget
            .retained_open_handles
            .checked_sub(session_handles_before)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let mut continuing = ready.ordinary_capture_budget();
        continuing.charge_source_bytes(0, 0, 0, session_bytes)?;
        continuing.retain_open_handles(session_handles)?;
        verified.shared_custody().admit_shared(&mut continuing)?;
        let disposal = verified.prepare_disposal(budget)?;
        ready.validate(budget)?;
        Ok((
            session,
            basis,
            disposal,
            continuing.retained_candidate_bytes,
            continuing.retained_open_handles,
        ))
    }

    pub(crate) fn validate(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeProjectError> {
        self.verified
            .validate_retained(&self.session.schema, &self.recovery, budget)?;
        self.session
            .startup_source_validation()
            .validate_with_budget(budget)?;
        Ok(())
    }

    /// Requires the exact publisher tied to this private Pending capture and nominal basis.
    /// The complete operation still holds Workspace/persistence guards across publication.
    pub(crate) fn bind(
        self,
        ready: BoundSourceUpgradeReady,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<BoundSourceUpgradeProject, SourceUpgradeProjectBindFailure> {
        let result = (|| {
            if &self.basis != ready.basis()
                || !Arc::ptr_eq(&self.session.schema, ready.observed_pending_schema())
            {
                return Err(SourceUpgradeProjectError::SchemaBasis);
            }
            self.validate(budget)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(BoundSourceUpgradeProject {
                project: self,
                ready,
            }),
            Err(error) => Err(SourceUpgradeProjectBindFailure {
                project: self,
                ready,
                error,
            }),
        }
    }

    fn install(
        self,
        ready: PublishedSourceUpgradeReady,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> PublishedSourceUpgradeProject {
        let Self {
            mut session,
            verified,
            recovery,
            disposal,
            retained_session_bytes,
            retained_session_handles,
            ..
        } = self;
        // The inventory owner discards its bodies and scan tables, refunds only that storage,
        // and moves the already charged native custody. Shared pins stay charged throughout.
        let custody = verified.into_custody(disposal, budget);
        session.install_source_observation_custody(custody);
        // Recovery is shared with the operation's storage/publication owners. Releasing this
        // reference does not authorize refunding their still-live encoded or decoded package.
        drop(recovery);

        // Document/asset source facts contain no duplicate Schema observations. Validation
        // derives manifest/bundle facts from this receipt, so moving the actual Ready receipt
        // updates that authority once. The private index needs only two Copy-field updates.
        session.source_index.commit_manifest_publication(
            ready.schema.manifest_identity(),
            ready.schema.manifest_digest(),
        );
        let (publication, usage) = ready.install_schema(&mut session.schema, budget);
        // Cumulative work/peak are operation evidence. These two fields describe the actual
        // continuing generation used by later validation/reclamation, not disposed candidates.
        session.usage = AuthoringProjectAdmissionUsage {
            retained_candidate_bytes: retained_session_bytes,
            retained_open_handles: retained_session_handles,
            ..usage
        };
        PublishedSourceUpgradeProject {
            session,
            publication,
        }
    }
}

/// One exclusive final publisher and its already prepared saved project generation.
pub(crate) struct BoundSourceUpgradeProject {
    project: PreparedSourceUpgradeProject,
    ready: BoundSourceUpgradeReady,
}

impl BoundSourceUpgradeProject {
    pub(in crate::project_host::authoring) fn cleanup_pending(&self) -> bool {
        self.ready.cleanup_pending()
    }

    pub(in crate::project_host::authoring) fn uncertain(&self) -> bool {
        self.ready.uncertain()
    }

    pub(in crate::project_host::authoring) fn retry_cleanup(&mut self) -> bool {
        self.ready.retry_cleanup()
    }

    #[cfg(test)]
    pub(in crate::project_host::authoring) fn test_fail_next_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        self.ready.test_fail_next_write()
    }

    pub(in crate::project_host::authoring) fn publish(
        self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<SourceUpgradeProjectOutcome, SourceUpgradeProjectPublicationRefusal> {
        // Readmission remains fallible before invoking the final publisher. It may observe
        // independent external edits; it does not claim to lock the whole project namespace.
        let Self { project, ready } = self;
        if let Err(error) = project.validate(budget) {
            return Err(SourceUpgradeProjectPublicationRefusal {
                bound: Self { project, ready },
                error,
            });
        }
        match ready.publish(budget) {
            Ok(SourceUpgradeReadyOutcome::Published(ready)) => Ok(
                SourceUpgradeProjectOutcome::Published(project.install(ready, budget)),
            ),
            Ok(SourceUpgradeReadyOutcome::Rejected { ready, reason }) => {
                Ok(SourceUpgradeProjectOutcome::Rejected {
                    bound: Self { project, ready },
                    reason,
                })
            }
            Ok(SourceUpgradeReadyOutcome::Failed { ready, stage }) => {
                Ok(SourceUpgradeProjectOutcome::Failed {
                    bound: Self { project, ready },
                    stage,
                })
            }
            Ok(SourceUpgradeReadyOutcome::Uncertain { ready, publication }) => {
                Ok(SourceUpgradeProjectOutcome::Uncertain {
                    bound: Self { project, ready },
                    publication,
                })
            }
            Err(refusal) => Err(SourceUpgradeProjectPublicationRefusal {
                bound: Self {
                    project,
                    ready: refusal.ready,
                },
                error: SourceUpgradeProjectError::Ready(refusal.error),
            }),
        }
    }
}

pub(crate) struct PublishedSourceUpgradeProject {
    pub(crate) session: AuthoringProjectSessionSnapshot,
    pub(crate) publication: FilePublicationEvidence,
}

#[must_use = "the complete operation must install the project or retain its publication facts"]
pub(crate) enum SourceUpgradeProjectOutcome {
    Published(PublishedSourceUpgradeProject),
    Rejected {
        bound: BoundSourceUpgradeProject,
        reason: FilePublicationRejection,
    },
    Failed {
        bound: BoundSourceUpgradeProject,
        stage: FilePublicationFailureStage,
    },
    Uncertain {
        bound: BoundSourceUpgradeProject,
        publication: FilePublicationEvidence,
    },
}

pub(crate) struct SourceUpgradeProjectBindFailure {
    pub(crate) project: PreparedSourceUpgradeProject,
    pub(crate) ready: BoundSourceUpgradeReady,
    pub(crate) error: SourceUpgradeProjectError,
}

/// Refusal preserves the already charged, move-only source proof for retry or explicit disposal.
pub(crate) struct SourceUpgradeProjectPreparationFailure {
    pub(crate) verified: VerifiedSourceUpgradeInventory,
    pub(crate) recovery: Arc<SourceUpgradeRecovery>,
    pub(crate) error: SourceUpgradeProjectError,
}

impl std::fmt::Debug for SourceUpgradeProjectPreparationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceUpgradeProjectPreparationFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

pub(crate) struct SourceUpgradeProjectPublicationRefusal {
    pub(crate) bound: BoundSourceUpgradeProject,
    pub(crate) error: SourceUpgradeProjectError,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceUpgradeProjectError {
    #[error("saved project preparation lost its private source storage")]
    PrivateStorage,
    #[error("the final publisher does not own this project's exact nominal Schema")]
    SchemaBasis,
    #[error("the complete successor source/metadata inventory no longer matches")]
    Inventory(SourceUpgradeInventoryError),
    #[error("project admission exceeded its shared allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("saved source or dependency preparation failed")]
    Project(#[from] AuthoringProjectSourceAdmissionError),
    #[error("the final Ready owner refused its project observations")]
    Ready(#[from] SourceUpgradeReadyError),
}

impl From<SourceUpgradeInventoryError> for SourceUpgradeProjectError {
    fn from(error: SourceUpgradeInventoryError) -> Self {
        Self::Inventory(error)
    }
}
