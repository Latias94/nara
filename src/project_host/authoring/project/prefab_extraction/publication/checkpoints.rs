//! One finite acceptance cursor over the original live checkpoints, with no new writes.

use super::*;

impl ProjectPrefabExtractionPublication {
    pub(super) fn accept_next_checkpoint(
        &mut self,
    ) -> Result<(), ProjectPrefabExtractionPublicationError> {
        #[cfg(feature = "tooling")]
        if let Origin::Live(storage) = &mut self.origin {
            // Q and metadata were not captured from an open document. Original entries follow
            // exactly the sparse original-output order used by the fixed Outputs table.
            let position = self.checkpoint_position.get_or_insert(2);
            let Some(entry) = self.entries.get_mut(*position) else {
                return Ok(());
            };
            let ExtractionFileMember::Original(source) = entry.member else {
                return Err(ProjectPrefabExtractionPublicationError::Phase);
            };
            let Expected::Existing { identity, .. } = entry.expected else {
                return Err(ProjectPrefabExtractionPublicationError::Phase);
            };
            storage.outputs_mut().accept_file_checkpoint(
                *position - 2,
                source,
                identity,
                &mut entry.result,
            )?;
            *position += 1;
            return Ok(());
        }
        // A fresh process has no old Workspace or nominal persistence checkpoints.
        self.checkpoint_position = Some(self.entries.len());
        Ok(())
    }
}
