//! UI-neutral image import settings, safe state projection, and Host action seams.

use std::sync::Arc;

use nara_asset::{AssetVersion, SourceHash, StableAssetId};

/// Exact revision of one selected image-import detail projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorImageImportDetailRevision(u64);

impl EditorImageImportDetailRevision {
    /// Creates a revision from the concrete Host projection clock.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Image color-space setting exposed without importing the image domain into tooling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorImageColorSpace {
    #[default]
    Srgb,
    Linear,
}

/// Complete editable image-import settings for the first Editor workflow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorImageImportSettings {
    color_space: EditorImageColorSpace,
}

impl EditorImageImportSettings {
    #[must_use]
    pub const fn new(color_space: EditorImageColorSpace) -> Self {
        Self { color_space }
    }

    #[must_use]
    pub const fn color_space(self) -> EditorImageColorSpace {
        self.color_space
    }
}

/// Safe metadata problem projected without raw source content or filesystem diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportMetadataIssue {
    SourceUnavailable,
    SourceChanged,
    MetadataUnavailable,
    MetadataChanged,
    InvalidFormat,
    PathMismatch,
    SourceKindMismatch,
    InvalidSettings,
}

/// Durable desired metadata state for one selected PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportMetadataView {
    Missing,
    Ready {
        stable_id: StableAssetId,
        committed_settings: EditorImageImportSettings,
    },
    Invalid {
        issue: EditorImageImportMetadataIssue,
    },
    IdentityConflict {
        stable_id: StableAssetId,
    },
}

impl EditorImageImportMetadataView {
    /// Returns the settings from which an Adapter-local draft must start.
    #[must_use]
    pub const fn draft_seed(self) -> Option<EditorImageImportSettings> {
        match self {
            Self::Missing => Some(EditorImageImportSettings::new(EditorImageColorSpace::Srgb)),
            Self::Ready {
                committed_settings, ..
            } => Some(committed_settings),
            Self::Invalid { .. } | Self::IdentityConflict { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_reimport(self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

/// Bounded image dimensions safe to expose to Editor presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorImageDimensions {
    width: u32,
    height: u32,
}

impl EditorImageDimensions {
    /// Creates dimensions from an accepted image stored by the concrete Host.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
}

/// Relationship between the accepted last-good image and the durable desired recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportFreshness {
    Fresh,
    SourceChanged,
    SettingsChanged,
    DesiredStateUnavailable,
}

/// Session-local accepted image state projected from the actual Host image store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportAvailabilityView {
    Unavailable,
    LastGood {
        stable_id: StableAssetId,
        version: AssetVersion,
        source_hash: SourceHash,
        active_settings: EditorImageImportSettings,
        dimensions: EditorImageDimensions,
        freshness: EditorImageImportFreshness,
    },
}

/// Honest pending phase exposed by the current concrete image transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportAttemptPhase {
    DecodeAndValidate,
}

/// Safe terminal failure category without backend error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportFailureReason {
    CatalogUnavailable,
    MetadataInvalid,
    IdentityUnavailable,
    CapacityExceeded,
    SourceUnavailable,
    MetadataPublication,
    ImageProcessing,
}

/// Latest attempt state for the selected image only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportAttemptView {
    Idle,
    Pending {
        generation: u64,
        phase: EditorImageImportAttemptPhase,
    },
    Succeeded {
        generation: u64,
    },
    Failed {
        generation: u64,
        reason: EditorImageImportFailureReason,
    },
    Stale {
        generation: u64,
    },
    Cancelled {
        generation: u64,
    },
    ReconcileRequired {
        generation: u64,
    },
}

impl EditorImageImportAttemptView {
    #[must_use]
    pub const fn blocks_submission(self) -> bool {
        matches!(self, Self::Pending { .. } | Self::ReconcileRequired { .. })
    }
}

/// One safe, selection-bound image import detail consumed by Editor Adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorImageImportView {
    relative_path: Arc<str>,
    detail_revision: EditorImageImportDetailRevision,
    metadata: EditorImageImportMetadataView,
    availability: EditorImageImportAvailabilityView,
    attempt: EditorImageImportAttemptView,
    submission_admitted: bool,
}

impl EditorImageImportView {
    /// Creates a projection from Host-owned current catalog and image-store facts.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        relative_path: Arc<str>,
        detail_revision: EditorImageImportDetailRevision,
        metadata: EditorImageImportMetadataView,
        availability: EditorImageImportAvailabilityView,
        attempt: EditorImageImportAttemptView,
        submission_admitted: bool,
    ) -> Self {
        Self {
            relative_path,
            detail_revision,
            metadata,
            availability,
            attempt,
            submission_admitted,
        }
    }

    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub const fn detail_revision(&self) -> EditorImageImportDetailRevision {
        self.detail_revision
    }

    #[must_use]
    pub const fn metadata(&self) -> EditorImageImportMetadataView {
        self.metadata
    }

    #[must_use]
    pub const fn availability(&self) -> EditorImageImportAvailabilityView {
        self.availability
    }

    #[must_use]
    pub const fn attempt(&self) -> EditorImageImportAttemptView {
        self.attempt
    }

    #[must_use]
    pub const fn draft_seed(&self) -> Option<EditorImageImportSettings> {
        self.metadata.draft_seed()
    }

    /// Reports whether this selected detail admits a submission.
    ///
    /// Adapters must also require [`crate::EditorFrameModel::image_import_work_pending`] to be
    /// false because the concrete Host owns one project-global attempt lane.
    #[must_use]
    pub const fn can_submit(&self) -> bool {
        self.submission_admitted && self.draft_seed().is_some() && !self.attempt.blocks_submission()
    }

    /// Creates the exact action for this immutable detail projection.
    #[must_use]
    pub fn submit_action(&self, settings: EditorImageImportSettings) -> EditorImageImportAction {
        EditorImageImportAction::Submit {
            relative_path: Arc::clone(&self.relative_path),
            expected_detail_revision: self.detail_revision,
            settings,
        }
    }
}

/// Exact-frame image import action handled by the concrete Editor Host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorImageImportAction {
    Submit {
        relative_path: Arc<str>,
        expected_detail_revision: EditorImageImportDetailRevision,
        settings: EditorImageImportSettings,
    },
}

/// Safe Host rejection for one admitted image import action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportActionRejection {
    Unavailable,
    Busy,
    Stale,
    MetadataBlocked,
    CapacityExceeded,
    ReconcileRequired,
}

/// Typed Host result for one accepted image import action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorImageImportActionOutcome {
    Started {
        generation: u64,
    },
    Completed {
        attempt: EditorImageImportAttemptView,
    },
    Rejected(EditorImageImportActionRejection),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(
        metadata: EditorImageImportMetadataView,
        attempt: EditorImageImportAttemptView,
    ) -> EditorImageImportView {
        EditorImageImportView::new(
            Arc::from("assets/icon.png"),
            EditorImageImportDetailRevision::new(17),
            metadata,
            EditorImageImportAvailabilityView::Unavailable,
            attempt,
            true,
        )
    }

    #[test]
    fn missing_metadata_seeds_srgb_without_committing_state() {
        let view = view(
            EditorImageImportMetadataView::Missing,
            EditorImageImportAttemptView::Idle,
        );

        assert_eq!(
            view.draft_seed(),
            Some(EditorImageImportSettings::new(EditorImageColorSpace::Srgb))
        );
        assert!(view.can_submit());
        assert_eq!(view.metadata(), EditorImageImportMetadataView::Missing);
    }

    #[test]
    fn invalid_metadata_and_active_work_block_submission() {
        let invalid = view(
            EditorImageImportMetadataView::Invalid {
                issue: EditorImageImportMetadataIssue::InvalidFormat,
            },
            EditorImageImportAttemptView::Idle,
        );
        let pending = view(
            EditorImageImportMetadataView::Missing,
            EditorImageImportAttemptView::Pending {
                generation: 3,
                phase: EditorImageImportAttemptPhase::DecodeAndValidate,
            },
        );
        let reconcile = view(
            EditorImageImportMetadataView::Missing,
            EditorImageImportAttemptView::ReconcileRequired { generation: 4 },
        );

        assert!(!invalid.can_submit());
        assert!(!pending.can_submit());
        assert!(!reconcile.can_submit());
    }

    #[test]
    fn terminal_failure_preserves_a_new_exact_submission() {
        let view = view(
            EditorImageImportMetadataView::Missing,
            EditorImageImportAttemptView::Failed {
                generation: 8,
                reason: EditorImageImportFailureReason::ImageProcessing,
            },
        );
        let settings = EditorImageImportSettings::new(EditorImageColorSpace::Linear);

        assert!(view.can_submit());
        assert_eq!(
            view.submit_action(settings),
            EditorImageImportAction::Submit {
                relative_path: Arc::from("assets/icon.png"),
                expected_detail_revision: EditorImageImportDetailRevision::new(17),
                settings,
            }
        );
    }
}
