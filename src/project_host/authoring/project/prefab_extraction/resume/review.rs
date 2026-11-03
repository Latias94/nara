//! Borrowed fixed data and actual inspection-time observations, without reconstructed receipts.

use nara_reflect::AuthoringSchemaBundle;
use nara_scene::{AuthoringSchemaBasis, SceneEntityId, SceneFileEncoding, SourceDocumentKey};

use super::super::{
    recovered_generation::RecoveredPrefabExtractionGeneration,
    recovery_data::{RecoveryFileState, RecoverySourceDisposition},
};
use crate::project_host::authoring::source_inventory::SourceFileObservation;

/// Current content at the captured name. This is not a past publication receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabExtractionRecoveryMemberState {
    Original,
    Output,
    Unchanged,
    NotCreated,
    MissingOriginal,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabExtractionRecoveryMemberDisposition {
    Guard,
    SaveCurrent,
    ReplaceSource,
    RewriteClosed,
}

/// Fixed original and reviewed inputs from the authenticated package, with a real native fact.
#[derive(Debug, Clone, Copy)]
pub struct PrefabExtractionRecoveryMember<'a> {
    pub key: &'a SourceDocumentKey,
    pub project_path: &'a str,
    pub encoding: SceneFileEncoding,
    pub explicit_native: bool,
    pub original_disk: &'a [u8],
    pub reviewed_working: &'a [u8],
    pub disposition: PrefabExtractionRecoveryMemberDisposition,
    pub output: Option<&'a [u8]>,
    pub observed: PrefabExtractionRecoveryMemberState,
}

#[derive(Debug, Clone, Copy)]
pub struct PrefabExtractionRecoveryDestination<'a> {
    pub key: &'a SourceDocumentKey,
    pub project_path: &'a str,
    pub encoding: SceneFileEncoding,
    pub output: &'a [u8],
    pub observed: PrefabExtractionRecoveryMemberState,
    pub metadata_path: &'a str,
    pub metadata_output: &'a [u8],
    pub metadata_observed: PrefabExtractionRecoveryMemberState,
}

/// Available only after complete stored-generation and saved-generation validation.
/// No input is freshly read or copied to expose this view.
pub struct PrefabExtractionRecoveryReview<'a> {
    generation: &'a RecoveredPrefabExtractionGeneration,
}

impl<'a> PrefabExtractionRecoveryReview<'a> {
    pub(super) fn new(generation: &'a RecoveredPrefabExtractionGeneration) -> Self {
        Self { generation }
    }

    pub fn schema_bundle(&self) -> &AuthoringSchemaBundle {
        self.generation.observation().models().bundle.as_ref()
    }

    pub fn schema_basis(&self) -> &AuthoringSchemaBasis {
        self.generation.observation().basis()
    }

    /// The profile stored by the original operation is the recovery's selected configuration.
    pub fn selected_profile(&self) -> Option<&str> {
        self.generation.pending().recovery().selected_profile()
    }

    pub fn selected_source(&self) -> &SourceDocumentKey {
        self.generation.pending().recovery().selection().source()
    }

    pub fn selected_root(&self) -> &SceneEntityId {
        self.generation.pending().recovery().selection().root()
    }

    pub fn unchanged_bundle_state(&self) -> PrefabExtractionRecoveryMemberState {
        state(
            self.generation
                .pending()
                .recovery()
                .unchanged_bundle()
                .classify(Some(self.generation.observation().bundle_digest())),
        )
    }

    pub fn destination(&self) -> PrefabExtractionRecoveryDestination<'_> {
        let data = self.generation.pending().recovery();
        let inventory = self.generation.observation().inventory();
        let digest = |path: &str| match inventory
            .file_observation(path)
            .expect("verified recovery retains both destination observations")
        {
            SourceFileObservation::Present { file, .. } => Some(file.digest()),
            SourceFileObservation::Missing { .. } => None,
        };
        PrefabExtractionRecoveryDestination {
            key: data.q().key(),
            project_path: data.q().path(),
            encoding: data.q().encoding().into(),
            output: data.q().output().bytes(),
            observed: state(data.q().classify(digest(data.q().path()))),
            metadata_path: data.q_sidecar().path(),
            metadata_output: data.q_sidecar().output().bytes(),
            metadata_observed: state(data.q_sidecar().classify(digest(data.q_sidecar().path()))),
        }
    }

    pub fn sources(
        &self,
    ) -> impl ExactSizeIterator<Item = PrefabExtractionRecoveryMember<'_>> + '_ {
        // Complete admission has already joined these sorted unique sets by exact source path.
        self.generation
            .pending()
            .recovery()
            .complete_original_sources()
            .iter()
            .zip(self.generation.observation().inventory().participants())
            .map(|(stored, actual)| PrefabExtractionRecoveryMember {
                key: stored.key(),
                project_path: stored.path(),
                encoding: stored.encoding().into(),
                explicit_native: stored.explicit_native(),
                original_disk: stored.original_disk().bytes(),
                reviewed_working: stored.reviewed_working().bytes(),
                disposition: match stored.disposition() {
                    RecoverySourceDisposition::Guard => {
                        PrefabExtractionRecoveryMemberDisposition::Guard
                    }
                    RecoverySourceDisposition::SaveCurrent => {
                        PrefabExtractionRecoveryMemberDisposition::SaveCurrent
                    }
                    RecoverySourceDisposition::ReplaceSource => {
                        PrefabExtractionRecoveryMemberDisposition::ReplaceSource
                    }
                    RecoverySourceDisposition::RewriteClosed => {
                        PrefabExtractionRecoveryMemberDisposition::RewriteClosed
                    }
                },
                output: stored.output().map(|blob| blob.bytes()),
                observed: state(stored.classify(Some(actual.file().digest()))),
            })
    }
}

fn state(value: RecoveryFileState) -> PrefabExtractionRecoveryMemberState {
    match value {
        RecoveryFileState::Original => PrefabExtractionRecoveryMemberState::Original,
        RecoveryFileState::Output => PrefabExtractionRecoveryMemberState::Output,
        RecoveryFileState::Unchanged => PrefabExtractionRecoveryMemberState::Unchanged,
        RecoveryFileState::NotCreated => PrefabExtractionRecoveryMemberState::NotCreated,
        RecoveryFileState::MissingOriginal => PrefabExtractionRecoveryMemberState::MissingOriginal,
        RecoveryFileState::Conflict => PrefabExtractionRecoveryMemberState::Conflict,
    }
}
