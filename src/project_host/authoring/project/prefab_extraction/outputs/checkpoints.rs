//! Accept exact persistence outputs after actual publication, retaining every refusal in place.

use super::super::publication::{ExtractionFileResult, ProjectPrefabExtractionPublicationError};
use super::*;
use crate::project_host::{
    authoring::source_inventory::{release, sum},
    persistence::ExtractionCheckpointAcceptance,
};

const OBSERVATION_SCRATCH: usize = 2 * 65_536;

impl PreparedProjectPrefabExtractionOutputs {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn accept_file_checkpoint(
        &mut self,
        output: usize,
        source: usize,
        previous: FileIdentity,
        result: &mut Option<ExtractionFileResult>,
    ) -> Result<(), ProjectPrefabExtractionPublicationError> {
        let member = self
            .members
            .get_mut(output)
            .ok_or(ProjectPrefabExtractionPublicationError::Phase)?;
        let ProjectPrefabExtractionOutputRole::Existing {
            source: actual,
            document,
        } = member.role
        else {
            return Err(ProjectPrefabExtractionPublicationError::Phase);
        };
        if actual != source {
            return Err(ProjectPrefabExtractionPublicationError::Phase);
        }
        let Some(document) = document else {
            return Ok(());
        };
        if let Some(Payload::Open(encoded)) = member.payload.as_ref() {
            if encoded.checkpoint().document() != document
                || member.digest != Some(encoded.digest())
                || result.as_ref().is_none_or(|file| {
                    matches!(file, ExtractionFileResult::Checkpoint(_))
                        || file.observation().2 != encoded.digest()
                })
            {
                return Err(ProjectPrefabExtractionPublicationError::Phase);
            }
            let Some(Payload::Open(encoded)) = member.payload.replace(Payload::Transferred) else {
                unreachable!("the original sealed checkpoint was checked above")
            };
            let acceptance = match result.take().expect("file result was checked above") {
                ExtractionFileResult::Published(publication) => {
                    ExtractionCheckpointAcceptance::published(encoded, previous, publication)
                }
                ExtractionFileResult::Observed { file, digest } => {
                    ExtractionCheckpointAcceptance::observed(encoded, file, digest)
                }
                ExtractionFileResult::Checkpoint(_) => {
                    unreachable!("the raw file result was checked above")
                }
            };
            *result = Some(ExtractionFileResult::Checkpoint(acceptance));
        }
        let Some(ExtractionFileResult::Checkpoint(acceptance)) = result.as_mut() else {
            return Err(ProjectPrefabExtractionPublicationError::Phase);
        };
        if !matches!(member.payload, Some(Payload::Transferred)) {
            return Err(ProjectPrefabExtractionPublicationError::Phase);
        }
        // Publication verification is constant-sized and consumes an actual receipt. An
        // already-current observation performs its own bounded digest read instead.
        if let Some(digest) = acceptance.observation_digest() {
            let length = usize::try_from(digest.length())
                .map_err(|_| SourceInventoryError::AccountingOverflow)?;
            let retained = self.review.context.budget.retained_candidate_bytes;
            let reservation = self.review.context.budget.charge_source_bytes(
                sum(length, 1)?,
                0,
                sum(length, 1)?,
                OBSERVATION_SCRATCH,
            );
            if let Err(error) = reservation {
                let reserved = self
                    .review
                    .context
                    .budget
                    .retained_candidate_bytes
                    .checked_sub(retained)
                    .ok_or(SourceInventoryError::AccountingOverflow)?;
                release(&mut self.review.context.budget, reserved)?;
                return Err(error.into());
            }
            let accepted = acceptance.advance(&mut self.review.persistence);
            release(&mut self.review.context.budget, OBSERVATION_SCRATCH)?;
            accepted.map_err(ProjectPrefabExtractionPublicationError::Checkpoint)?;
        } else {
            acceptance
                .advance(&mut self.review.persistence)
                .map_err(ProjectPrefabExtractionPublicationError::Checkpoint)?;
        }
        Ok(())
    }
}
