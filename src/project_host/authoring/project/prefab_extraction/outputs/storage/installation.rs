//! One-way completed-file boundary: move the original Workspace stage out of native custody.
//!
//! The returned Workspace stage can enter its own borrowing guard. Native validation and final
//! publication retain everything they need independently, including the original nominal basis.

use super::super::super::publication::{PublicationBundle, PublicationInputs, PublicationSchema};
use super::*;

pub(in crate::project_host::authoring::project::prefab_extraction) struct LivePrefabExtractionInstallation
{
    context: ProjectExtractionContext,
    pending: PendingRecoveryStorage<PrefabExtractionRecovery>,
    // Keep all already charged semantic/data owners until final installation or refusal ends.
    _saved: PrefabExtractionSavedGeneration,
    _recovery: Option<Arc<PrefabExtractionRecovery>>,
    _buffers: CompletedBuffers,
    _orphan: Option<FilePublicationEvidence>,
}

struct CompletedBuffers {
    _members: Vec<super::super::OutputMember>,
    _working: Vec<Vec<u8>>,
    _encodings: Vec<SceneFileEncoding>,
    _unshared_recovery: Option<PrefabExtractionRecovery>,
}

impl ProjectPrefabExtractionStorage {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_installation(
        mut self,
    ) -> Result<
        (
            LivePrefabExtractionInstallation,
            PreparedEditorWorkspaceExtractionPersistence,
        ),
        ProjectPrefabExtractionStorageFailure,
    > {
        let result = (|| {
            if !matches!(self.stage, Stage::Confirmed(_)) || self.uncertain_publication.is_some() {
                return Err(ProjectPrefabExtractionStorageError::InstallationPhase);
            }
            let review = &self.owner.outputs.review;
            if review.context.basis
                != *review
                    .persistence
                    .sources()
                    .prepared()
                    .corpus()
                    .schema_basis()
            {
                return Err(ProjectPrefabExtractionStorageError::InstallationPhase);
            }
            // The payload vectors themselves move without copying. Admit the new aggregate
            // metadata before consuming the old wrappers; keep their original reservation.
            let bytes = size_of::<LivePrefabExtractionInstallation>();
            let budget = &mut self.owner.outputs.review.context.budget;
            let before = budget.retained_candidate_bytes;
            if let Err(error) = budget.charge_source_bytes(0, 0, bytes, bytes) {
                let unused = budget
                    .retained_candidate_bytes
                    .checked_sub(before)
                    .expect("refused installation metadata has allocated no storage");
                budget
                    .reconcile_retained_reservation(unused, 0)
                    .expect("original completed publication remains charged");
                return Err(RecoveryStorageError::Admission(error).into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            return Err(ProjectPrefabExtractionStorageFailure {
                storage: self,
                error,
            });
        }
        let Self {
            owner,
            recovery,
            stage,
            orphan_publication,
            ..
        } = self;
        let Stage::Confirmed(pending) = stage else {
            unreachable!("completed installation phase was checked before moving owners")
        };
        let working = owner.working;
        let encodings = owner.encodings;
        let unshared_recovery = owner.recovery;
        let PreparedProjectPrefabExtractionOutputs {
            review, members, ..
        } = owner.outputs;
        let PreparedProjectPrefabExtractionReview {
            context,
            persistence,
            saved,
        } = review;
        Ok((
            LivePrefabExtractionInstallation {
                context,
                pending,
                _saved: saved,
                _recovery: recovery,
                _buffers: CompletedBuffers {
                    _members: members,
                    _working: working,
                    _encodings: encodings,
                    _unshared_recovery: unshared_recovery,
                },
                _orphan: orphan_publication,
            },
            persistence,
        ))
    }
}

impl LivePrefabExtractionInstallation {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn session_context(
        &self,
    ) -> &ProjectExtractionContext {
        &self.context
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn session_context_mut(
        &mut self,
    ) -> &mut ProjectExtractionContext {
        &mut self.context
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn publication_inputs(
        &mut self,
    ) -> PublicationInputs<'_> {
        let context = &mut self.context;
        PublicationInputs {
            pending: self.pending.observed(),
            inventory: &context.inventory,
            budget: &mut context.budget,
            original_open: Some(&context.open),
            bundle: PublicationBundle {
                path: context.schema.bundle_path().as_str(),
                identity: context.schema.bundle_identity(),
                parent: context.schema.bundle_parent_identity(),
                digest: context.schema.bundle_digest(),
            },
            schema: PublicationSchema::Live {
                receipt: &context.schema,
                adapters: &context.adapters,
                basis: context.basis.clone(),
            },
        }
    }
}
