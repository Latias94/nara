use std::sync::Arc;

use nara_asset::SourceHash;
use nara_image::{ImageColorSpace, ImageImportSettings};
use nara_tooling::{
    EditorImageColorSpace, EditorImageDimensions, EditorImageImportActionOutcome,
    EditorImageImportActionRejection, EditorImageImportAttemptPhase, EditorImageImportAttemptView,
    EditorImageImportAvailabilityView, EditorImageImportDetailRevision,
    EditorImageImportFailureReason, EditorImageImportFreshness, EditorImageImportMetadataIssue,
    EditorImageImportMetadataView, EditorImageImportSettings as SafeImageImportSettings,
    EditorImageImportView,
};

use crate::project_host::authoring::{ProjectSourceIndexGeneration, ProjectSourceIndexSnapshot};

use super::{
    catalog::{EditorImageMetadataInvalidity, EditorImageMetadataStatus},
    store::EditorImageSourceProof,
    transaction::{
        EditorImageImportFailure, EditorImageImportFrame, EditorImageImportHost,
        EditorImageImportLatestDetail, EditorImageImportLatestMetadata,
        EditorImageImportRequestResult, EditorImageImportState,
    },
};
use crate::project_host::runtime::editor::authoring_runtime::{
    EditorAuthoringRuntime, EditorImageRuntimeFailure,
};

pub(crate) struct EditorImageImportProjection {
    frame: Option<EditorImageImportFrame>,
    view: EditorImageImportView,
}

#[derive(Clone, Copy)]
enum EditorImageSourceObservation {
    Verified {
        identity: nara_fs::FileIdentity,
        digest: nara_fs::ContentDigest,
    },
    Unverified,
    Changed,
    Unavailable,
}

impl EditorImageImportProjection {
    #[must_use]
    pub(crate) fn into_parts(self) -> (Option<EditorImageImportFrame>, EditorImageImportView) {
        (self.frame, self.view)
    }
}

impl EditorImageImportHost {
    #[must_use]
    pub(crate) fn projection_for(
        &self,
        source_index: &Arc<ProjectSourceIndexSnapshot>,
        relative_path: &str,
        runtime: &EditorAuthoringRuntime,
    ) -> Option<EditorImageImportProjection> {
        if let Some(frame) = self.frame_for(source_index, relative_path) {
            return Some(self.project_catalog_detail(frame, runtime));
        }
        let detail = self.detail_for(relative_path)?;
        Some(self.project_accepted_detail(detail, runtime))
    }

    fn project_catalog_detail(
        &self,
        frame: EditorImageImportFrame,
        runtime: &EditorAuthoringRuntime,
    ) -> EditorImageImportProjection {
        let entry = frame
            .entry()
            .expect("an image detail frame retains its admitted catalog entry");
        let identity_conflict =
            self.last_good(entry.asset_path())
                .and_then(|last_good| match entry.metadata() {
                    EditorImageMetadataStatus::Missing
                        if frame.contains_stable_id(last_good.stable_id()) =>
                    {
                        Some(last_good.stable_id())
                    }
                    EditorImageMetadataStatus::Ready(receipt)
                        if receipt.stable_id() != last_good.stable_id() =>
                    {
                        Some(receipt.stable_id())
                    }
                    EditorImageMetadataStatus::Missing
                    | EditorImageMetadataStatus::Ready(_)
                    | EditorImageMetadataStatus::Invalid(_)
                    | EditorImageMetadataStatus::IdentityConflict { .. } => None,
                });
        let metadata = identity_conflict.map_or_else(
            || project_metadata(entry.metadata()),
            |stable_id| EditorImageImportMetadataView::IdentityConflict { stable_id },
        );
        let desired = match (identity_conflict, entry.metadata()) {
            (None, EditorImageMetadataStatus::Ready(receipt)) => {
                Some((receipt.stable_id(), receipt.settings()))
            }
            (Some(_), _)
            | (None, EditorImageMetadataStatus::Missing)
            | (None, EditorImageMetadataStatus::Invalid(_))
            | (None, EditorImageMetadataStatus::IdentityConflict { .. }) => None,
        };
        let attempt = self.attempt_for(frame.relative_path());
        let availability = self.project_availability(
            entry.asset_path(),
            entry
                .source_identity()
                .map(|identity| (identity, frame.source_index_generation())),
            source_conflict_observed(attempt, Some(entry.metadata())),
            desired,
            runtime,
        );
        let view = EditorImageImportView::new(
            Arc::from(frame.relative_path()),
            EditorImageImportDetailRevision::new(frame.detail_revision()),
            metadata,
            availability,
            project_attempt(attempt),
            true,
        );
        EditorImageImportProjection {
            frame: Some(frame),
            view,
        }
    }

    fn project_accepted_detail(
        &self,
        detail: &EditorImageImportLatestDetail,
        runtime: &EditorAuthoringRuntime,
    ) -> EditorImageImportProjection {
        let (metadata, desired) = match detail.metadata() {
            EditorImageImportLatestMetadata::Ready => (
                EditorImageImportMetadataView::Ready {
                    stable_id: detail.stable_id(),
                    committed_settings: project_settings(detail.settings()),
                },
                Some((detail.stable_id(), detail.settings())),
            ),
            EditorImageImportLatestMetadata::Uncertain => (
                EditorImageImportMetadataView::Invalid {
                    issue: EditorImageImportMetadataIssue::MetadataUnavailable,
                },
                None,
            ),
        };
        let attempt = self.attempt_for(detail.relative_path());
        let availability = self.project_availability(
            detail.asset_path(),
            None,
            source_conflict_observed(attempt, None),
            desired,
            runtime,
        );
        let view = EditorImageImportView::new(
            Arc::from(detail.relative_path()),
            EditorImageImportDetailRevision::new(detail.detail_revision()),
            metadata,
            availability,
            project_attempt(attempt),
            false,
        );
        EditorImageImportProjection { frame: None, view }
    }

    fn project_availability(
        &self,
        asset_path: &nara_asset::AssetPath,
        current_source: Option<(nara_fs::FileIdentity, ProjectSourceIndexGeneration)>,
        source_conflict_observed: bool,
        desired: Option<(nara_asset::StableAssetId, ImageImportSettings)>,
        runtime: &EditorAuthoringRuntime,
    ) -> EditorImageImportAvailabilityView {
        self.last_good(asset_path)
            .and_then(|last_good| {
                let image = runtime.image_import_asset(last_good.handle(), last_good.version())?;
                let expected_source_hash =
                    SourceHash::from_digest(*last_good.source_digest().as_bytes());
                if image.source().stable_id() != last_good.stable_id()
                    || image.source().path() != asset_path
                    || image.source().source_hash() != expected_source_hash
                    || image.source().settings().settings_hash() != last_good.settings_hash()
                {
                    return None;
                }
                let source_observation = match current_source {
                    _ if source_conflict_observed => EditorImageSourceObservation::Changed,
                    Some((identity, _)) if identity != last_good.source_identity() => {
                        EditorImageSourceObservation::Changed
                    }
                    Some((identity, generation)) => match last_good.source_proof() {
                        EditorImageSourceProof::SourceIndex {
                            generation: admitted,
                        } if generation == admitted => EditorImageSourceObservation::Verified {
                            identity,
                            digest: last_good.source_digest(),
                        },
                        EditorImageSourceProof::SourceIndex { .. }
                        | EditorImageSourceProof::AuthoringClosure { .. } => {
                            EditorImageSourceObservation::Unverified
                        }
                    },
                    None => EditorImageSourceObservation::Unavailable,
                };
                let freshness = match (source_observation, desired) {
                    (EditorImageSourceObservation::Changed, _) => {
                        EditorImageImportFreshness::SourceChanged
                    }
                    (EditorImageSourceObservation::Unverified, _) => {
                        EditorImageImportFreshness::SourceUnverified
                    }
                    (EditorImageSourceObservation::Unavailable, _) => {
                        EditorImageImportFreshness::DesiredStateUnavailable
                    }
                    (EditorImageSourceObservation::Verified { identity, digest }, _)
                        if identity != last_good.source_identity()
                            || digest != last_good.source_digest() =>
                    {
                        EditorImageImportFreshness::SourceChanged
                    }
                    (
                        EditorImageSourceObservation::Verified { .. },
                        Some((stable_id, settings)),
                    ) if stable_id == last_good.stable_id() => {
                        if settings.settings_hash() == last_good.settings_hash() {
                            EditorImageImportFreshness::Fresh
                        } else {
                            EditorImageImportFreshness::SettingsChanged
                        }
                    }
                    (EditorImageSourceObservation::Verified { .. }, _) => {
                        EditorImageImportFreshness::DesiredStateUnavailable
                    }
                };
                let extent = image.extent();
                Some(EditorImageImportAvailabilityView::LastGood {
                    stable_id: last_good.stable_id(),
                    version: last_good.version(),
                    source_hash: image.source().source_hash(),
                    active_settings: project_settings(image.source().settings()),
                    dimensions: EditorImageDimensions::new(extent.width, extent.height),
                    freshness,
                })
            })
            .unwrap_or(EditorImageImportAvailabilityView::Unavailable)
    }
}

fn source_conflict_observed(
    attempt: EditorImageImportState,
    metadata: Option<&EditorImageMetadataStatus>,
) -> bool {
    matches!(
        attempt,
        EditorImageImportState::Failed {
            failure: EditorImageImportFailure::SourceUnavailable
                | EditorImageImportFailure::SourceChanged
                | EditorImageImportFailure::Runtime(EditorImageRuntimeFailure::SourceChanged),
            ..
        }
    ) || matches!(
        metadata,
        Some(EditorImageMetadataStatus::Invalid(
            EditorImageMetadataInvalidity::SourceIdentityUnavailable
                | EditorImageMetadataInvalidity::SourceUnavailable
                | EditorImageMetadataInvalidity::SourceIdentityDrift
        ))
    )
}

#[must_use]
pub(in crate::project_host::runtime::editor) const fn domain_settings(
    settings: SafeImageImportSettings,
) -> ImageImportSettings {
    let color_space = match settings.color_space() {
        EditorImageColorSpace::Srgb => ImageColorSpace::Srgb,
        EditorImageColorSpace::Linear => ImageColorSpace::Linear,
    };
    ImageImportSettings::new(color_space)
}

#[must_use]
pub(in crate::project_host::runtime::editor) fn project_request_outcome(
    result: EditorImageImportRequestResult,
) -> EditorImageImportActionOutcome {
    match result {
        EditorImageImportRequestResult::Started(generation) => {
            EditorImageImportActionOutcome::Started {
                generation: generation.get(),
            }
        }
        EditorImageImportRequestResult::Completed(state) => {
            EditorImageImportActionOutcome::Completed {
                attempt: project_attempt(state),
            }
        }
        EditorImageImportRequestResult::Rejected(failure) => {
            EditorImageImportActionOutcome::Rejected(project_rejection(failure))
        }
    }
}

fn project_metadata(metadata: &EditorImageMetadataStatus) -> EditorImageImportMetadataView {
    match metadata {
        EditorImageMetadataStatus::Missing => EditorImageImportMetadataView::Missing,
        EditorImageMetadataStatus::Ready(receipt) => EditorImageImportMetadataView::Ready {
            stable_id: receipt.stable_id(),
            committed_settings: project_settings(receipt.settings()),
        },
        EditorImageMetadataStatus::Invalid(issue) => EditorImageImportMetadataView::Invalid {
            issue: project_metadata_issue(*issue),
        },
        EditorImageMetadataStatus::IdentityConflict { stable_id } => {
            EditorImageImportMetadataView::IdentityConflict {
                stable_id: *stable_id,
            }
        }
    }
}

const fn project_metadata_issue(
    issue: EditorImageMetadataInvalidity,
) -> EditorImageImportMetadataIssue {
    match issue {
        EditorImageMetadataInvalidity::SourceIdentityUnavailable
        | EditorImageMetadataInvalidity::SourceUnavailable => {
            EditorImageImportMetadataIssue::SourceUnavailable
        }
        EditorImageMetadataInvalidity::SourceIdentityDrift => {
            EditorImageImportMetadataIssue::SourceChanged
        }
        EditorImageMetadataInvalidity::MetadataIdentityUnavailable
        | EditorImageMetadataInvalidity::MetadataUnavailable => {
            EditorImageImportMetadataIssue::MetadataUnavailable
        }
        EditorImageMetadataInvalidity::MetadataIdentityDrift => {
            EditorImageImportMetadataIssue::MetadataChanged
        }
        EditorImageMetadataInvalidity::MetadataFormat => {
            EditorImageImportMetadataIssue::InvalidFormat
        }
        EditorImageMetadataInvalidity::MetadataPathMismatch => {
            EditorImageImportMetadataIssue::PathMismatch
        }
        EditorImageMetadataInvalidity::MetadataSourceKindMismatch => {
            EditorImageImportMetadataIssue::SourceKindMismatch
        }
        EditorImageMetadataInvalidity::ImageSettings => {
            EditorImageImportMetadataIssue::InvalidSettings
        }
    }
}

const fn project_settings(settings: ImageImportSettings) -> SafeImageImportSettings {
    let color_space = match settings.color_space() {
        ImageColorSpace::Srgb => EditorImageColorSpace::Srgb,
        ImageColorSpace::Linear => EditorImageColorSpace::Linear,
    };
    SafeImageImportSettings::new(color_space)
}

fn project_attempt(state: EditorImageImportState) -> EditorImageImportAttemptView {
    match state {
        EditorImageImportState::Idle => EditorImageImportAttemptView::Idle,
        EditorImageImportState::Pending { generation } => EditorImageImportAttemptView::Pending {
            generation: generation.get(),
            phase: EditorImageImportAttemptPhase::DecodeAndValidate,
        },
        EditorImageImportState::Succeeded { generation, .. } => {
            EditorImageImportAttemptView::Succeeded {
                generation: generation.get(),
            }
        }
        EditorImageImportState::Failed {
            generation,
            failure:
                EditorImageImportFailure::StaleFrame
                | EditorImageImportFailure::SourceChanged
                | EditorImageImportFailure::Runtime(EditorImageRuntimeFailure::SourceChanged),
        } => EditorImageImportAttemptView::Stale {
            generation: generation.get(),
        },
        EditorImageImportState::Failed {
            generation,
            failure: EditorImageImportFailure::Runtime(EditorImageRuntimeFailure::TaskCancelled),
        } => EditorImageImportAttemptView::Cancelled {
            generation: generation.get(),
        },
        EditorImageImportState::Failed {
            generation,
            failure,
        } => EditorImageImportAttemptView::Failed {
            generation: generation.get(),
            reason: project_failure_reason(failure),
        },
        EditorImageImportState::ReconcileRequired { generation } => {
            EditorImageImportAttemptView::ReconcileRequired {
                generation: generation.get(),
            }
        }
    }
}

fn project_failure_reason(failure: EditorImageImportFailure) -> EditorImageImportFailureReason {
    match failure {
        EditorImageImportFailure::CatalogUnavailable
        | EditorImageImportFailure::CatalogRejected => {
            EditorImageImportFailureReason::CatalogUnavailable
        }
        EditorImageImportFailure::MetadataInvalid => {
            EditorImageImportFailureReason::MetadataInvalid
        }
        EditorImageImportFailure::StableIdUnavailable => {
            EditorImageImportFailureReason::IdentityUnavailable
        }
        EditorImageImportFailure::LastGoodLimitExceeded => {
            EditorImageImportFailureReason::CapacityExceeded
        }
        EditorImageImportFailure::SourceUnavailable
        | EditorImageImportFailure::SourceChanged
        | EditorImageImportFailure::Runtime(EditorImageRuntimeFailure::SourceChanged) => {
            EditorImageImportFailureReason::SourceUnavailable
        }
        EditorImageImportFailure::MetadataEncoding
        | EditorImageImportFailure::MetadataPublicationRejected
        | EditorImageImportFailure::MetadataPublicationFailed
        | EditorImageImportFailure::MetadataPublicationUncertain => {
            EditorImageImportFailureReason::MetadataPublication
        }
        EditorImageImportFailure::Busy
        | EditorImageImportFailure::ReconcileRequired
        | EditorImageImportFailure::StaleFrame
        | EditorImageImportFailure::EntryUnavailable
        | EditorImageImportFailure::Runtime(
            EditorImageRuntimeFailure::RuntimeUnavailable
            | EditorImageRuntimeFailure::AssetIdentity
            | EditorImageRuntimeFailure::ImportAdmission
            | EditorImageRuntimeFailure::TaskAdmission
            | EditorImageRuntimeFailure::Import
            | EditorImageRuntimeFailure::TaskCancelled
            | EditorImageRuntimeFailure::TaskFailed
            | EditorImageRuntimeFailure::Publication,
        )
        | EditorImageImportFailure::ResultMismatch => {
            EditorImageImportFailureReason::ImageProcessing
        }
    }
}

fn project_rejection(failure: EditorImageImportFailure) -> EditorImageImportActionRejection {
    match failure {
        EditorImageImportFailure::Busy => EditorImageImportActionRejection::Busy,
        EditorImageImportFailure::ReconcileRequired => {
            EditorImageImportActionRejection::ReconcileRequired
        }
        EditorImageImportFailure::StaleFrame | EditorImageImportFailure::SourceChanged => {
            EditorImageImportActionRejection::Stale
        }
        EditorImageImportFailure::MetadataInvalid
        | EditorImageImportFailure::StableIdUnavailable => {
            EditorImageImportActionRejection::MetadataBlocked
        }
        EditorImageImportFailure::LastGoodLimitExceeded => {
            EditorImageImportActionRejection::CapacityExceeded
        }
        EditorImageImportFailure::CatalogUnavailable
        | EditorImageImportFailure::CatalogRejected
        | EditorImageImportFailure::EntryUnavailable
        | EditorImageImportFailure::SourceUnavailable
        | EditorImageImportFailure::MetadataEncoding
        | EditorImageImportFailure::MetadataPublicationRejected
        | EditorImageImportFailure::MetadataPublicationFailed
        | EditorImageImportFailure::MetadataPublicationUncertain
        | EditorImageImportFailure::Runtime(_)
        | EditorImageImportFailure::ResultMismatch => EditorImageImportActionRejection::Unavailable,
    }
}
