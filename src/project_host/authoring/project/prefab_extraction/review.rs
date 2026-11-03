//! Fixed Review selection and its independently proved saved generation.

use nara_tooling::{EditorPrefabExtractionDisposition, EditorWorkspaceCommandReport};

use super::*;

pub(crate) struct PreparedProjectPrefabExtractionReview {
    pub(super) context: ProjectExtractionContext,
    pub(super) persistence: PreparedEditorWorkspaceExtractionPersistence,
    pub(super) saved: PrefabExtractionSavedGeneration,
}

pub(crate) enum ProjectPrefabExtractionReviewFailure {
    Sources {
        context: ProjectExtractionContext,
        failure: EditorWorkspacePrefabExtractionSourcePrepareFailure,
    },
    Persistence {
        context: ProjectExtractionContext,
        failure: EditorWorkspaceExtractionPersistencePrepareFailure,
    },
    Saved {
        context: ProjectExtractionContext,
        persistence: PreparedEditorWorkspaceExtractionPersistence,
        error: EditorWorkspaceExtractionSavedGenerationFailure,
    },
    Revision {
        review: PreparedProjectPrefabExtractionReview,
    },
}

impl fmt::Debug for PreparedProjectPrefabExtractionReview {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedProjectPrefabExtractionReview")
            .field("saved", &self.saved)
            .field("persistence", &self.persistence)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for ProjectPrefabExtractionReviewFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sources { failure, .. } => formatter
                .debug_tuple("Sources")
                .field(failure.report())
                .finish(),
            Self::Persistence { failure, .. } => formatter
                .debug_tuple("Persistence")
                .field(failure.report())
                .finish(),
            Self::Saved { error, .. } => formatter.debug_tuple("Saved").field(error).finish(),
            Self::Revision { .. } => formatter.write_str("Revision"),
        }
    }
}

impl PreparedProjectPrefabExtraction {
    /// Source/history and saved checkpoints are fixed before the final saved-source proof.
    /// Refusal retains those same owners; no live source, saved field or file is changed.
    pub(crate) fn prepare_review(
        self,
        save_current: Vec<EditorDocumentId>,
    ) -> Result<PreparedProjectPrefabExtractionReview, ProjectPrefabExtractionReviewFailure> {
        let Self { context, workspace } = self;
        let sources = match workspace.prepare_sources() {
            Ok(sources) => sources,
            Err(failure) => {
                return Err(ProjectPrefabExtractionReviewFailure::Sources { context, failure });
            }
        };
        prepare_persistence(context, sources, save_current)
    }
}

fn prepare_persistence(
    context: ProjectExtractionContext,
    sources: nara_tooling::PreparedEditorWorkspaceExtractionSources,
    save_current: Vec<EditorDocumentId>,
) -> Result<PreparedProjectPrefabExtractionReview, ProjectPrefabExtractionReviewFailure> {
    let mut persistence = match sources.prepare_persistence(save_current) {
        Ok(persistence) => persistence,
        Err(failure) => {
            return Err(ProjectPrefabExtractionReviewFailure::Persistence { context, failure });
        }
    };
    match persistence.prepare_saved_generation(&context.disk) {
        Ok(saved) => Ok(PreparedProjectPrefabExtractionReview {
            context,
            persistence,
            saved,
        }),
        Err(error) => Err(ProjectPrefabExtractionReviewFailure::Saved {
            context,
            persistence,
            error,
        }),
    }
}

impl ProjectPrefabExtractionReviewFailure {
    /// A corrected explicit selection continues the same physical corpus and spent Work.
    pub(crate) fn retry(
        self,
        save_current: Vec<EditorDocumentId>,
    ) -> Result<PreparedProjectPrefabExtractionReview, Self> {
        match self {
            Self::Sources { context, failure } => {
                let (workspace, _) = failure.into_parts();
                PreparedProjectPrefabExtraction { context, workspace }.prepare_review(save_current)
            }
            Self::Persistence { context, failure } => {
                let (sources, _, _) = failure.into_parts();
                prepare_persistence(context, sources, save_current)
            }
            Self::Saved {
                context,
                persistence,
                error,
            } => match persistence.into_sources_before_publication() {
                Ok(sources) => prepare_persistence(context, sources, save_current),
                Err(persistence) => Err(Self::Saved {
                    context,
                    persistence,
                    error,
                }),
            },
            Self::Revision { review } => review.revise(save_current),
        }
    }

    /// Restores a valid Review after a refused selection change without losing its proof or
    /// accepted publication evidence. Other preparation failures have no valid Review to restore.
    pub(crate) fn into_review(self) -> Result<PreparedProjectPrefabExtractionReview, Self> {
        match self {
            Self::Revision { review } => Ok(review),
            failure => Err(failure),
        }
    }
}

impl PreparedProjectPrefabExtractionReview {
    /// Revises only the unpublished save selection, retaining the captured corpus and Work.
    /// An outstanding checkpoint or publication result must return to its original owner first.
    pub(crate) fn revise(
        self,
        save_current: Vec<EditorDocumentId>,
    ) -> Result<Self, ProjectPrefabExtractionReviewFailure> {
        let Self {
            context,
            persistence,
            saved,
        } = self;
        match persistence.into_sources_before_publication() {
            Ok(sources) => prepare_persistence(context, sources, save_current),
            Err(persistence) => Err(ProjectPrefabExtractionReviewFailure::Revision {
                review: Self {
                    context,
                    persistence,
                    saved,
                },
            }),
        }
    }

    pub(crate) fn documents(
        &self,
    ) -> impl ExactSizeIterator<Item = (EditorDocumentId, EditorPrefabExtractionDisposition)> + '_
    {
        self.persistence.documents()
    }

    pub(crate) fn saved_generation(&self) -> &PrefabExtractionSavedGeneration {
        &self.saved
    }

    pub(crate) fn require_saved_generation(&mut self) -> Result<(), EditorWorkspaceCommandReport> {
        self.persistence.require_saved_generation(&self.saved)
    }

    pub(crate) fn revalidate_project(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
    ) -> Result<(), PrefabExtractionProjectError> {
        self.context.validate(project)
    }
}
