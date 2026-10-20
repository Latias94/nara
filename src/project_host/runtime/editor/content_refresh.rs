use nara_asset::AssetPath;
use nara_fs::ContentDigest;
use nara_tooling::{
    EditorDocumentDigest, EditorDocumentId, EditorProjectContentAction,
    EditorProjectContentRefreshFailure, EditorProjectContentRefreshRejection,
    EditorProjectContentRefreshRequestResult, EditorProjectContentView,
};

use crate::project_host::runtime_agreement::prepare_runtime_schema_agreement;
use crate::project_host::{ProjectContentError, ProjectContentErrorKind, StartupContentMemberKind};

use super::{
    EditorPersistenceReceipt, EditorProjectContentGeneration, EditorProjectSession,
    EditorRuntimeOwnerState, PendingProjectContentRefresh, single_error,
};

impl EditorProjectSession {
    pub(super) fn schedule_prefab_content_refresh(&mut self, evidence: EditorPersistenceReceipt) {
        let document = evidence.document();
        let prefab = self
            .workspace
            .prefab(document)
            .expect("an accepted Prefab save still belongs to its exact workspace slot");
        let path = AssetPath::new(prefab.source_binding().locator().to_owned())
            .expect("a Host-normalized Prefab source remains a valid asset path");
        let saved_digest = evidence
            .published_digest()
            .expect("an accepted Prefab save carries verified published bytes");
        self.queue_prefab_content_refresh(
            document,
            evidence.revision(),
            path,
            saved_digest,
            ContentDigest::from_parts(saved_digest.encoded_bytes(), saved_digest.hash()),
        );
    }

    pub(super) fn schedule_reopened_prefab_content_refresh(
        &mut self,
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        path: AssetPath,
        digest: EditorDocumentDigest,
        content_digest: ContentDigest,
    ) {
        self.queue_prefab_content_refresh(document, revision, path, digest, content_digest);
    }

    fn queue_prefab_content_refresh(
        &mut self,
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        prefab: AssetPath,
        saved_digest: EditorDocumentDigest,
        expected_digest: ContentDigest,
    ) {
        let Some(current_generation) = self.project_content.current_generation() else {
            return;
        };
        if self.current_snapshot_has_prefab_digest(&prefab, expected_digest) {
            self.pending_content_refresh = None;
            self.failed_content_refresh = None;
            self.project_content = EditorProjectContentView::Current {
                generation: current_generation,
            };
            return;
        }
        let request = PendingProjectContentRefresh {
            document,
            revision,
            prefab,
            saved_digest,
            expected_digest,
        };
        self.project_content = refresh_pending_view(current_generation, &request);
        self.pending_content_refresh = Some(request);
        self.failed_content_refresh = None;
    }

    fn current_snapshot_has_prefab_digest(
        &self,
        prefab: &AssetPath,
        digest: ContentDigest,
    ) -> bool {
        let Some(lane) = self.trusted_compiled() else {
            return false;
        };
        let path = format!(
            "{}/{}",
            lane.content_candidate.settings().paths.prefabs.as_str(),
            prefab.as_str()
        );
        lane.snapshot
            .startup_content_closure()
            .members()
            .iter()
            .any(|member| {
                member.kind() == StartupContentMemberKind::Prefab
                    && member.path().as_str() == path
                    && member.digest() == digest
            })
    }

    pub(super) fn request_project_content_refresh(
        &mut self,
        action: EditorProjectContentAction,
    ) -> EditorProjectContentRefreshRequestResult {
        if self.trusted_compiled().is_none() {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RetryNotAvailable,
            );
        }
        match action {
            EditorProjectContentAction::RetryRefresh => {}
        }
        if self.pending_content_refresh.is_some() {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RefreshPending,
            );
        }
        if self.project_content.is_current() {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::ContentAlreadyCurrent,
            );
        }
        if !self.project_content.retryable() {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RetryNotAvailable,
            );
        }
        let Some(request) = self.failed_content_refresh.take() else {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RetryNotAvailable,
            );
        };
        let Some(current_generation) = self.project_content.current_generation() else {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RetryNotAvailable,
            );
        };
        self.project_content = refresh_pending_view(current_generation, &request);
        self.pending_content_refresh = Some(request);
        EditorProjectContentRefreshRequestResult::Accepted
    }

    pub(super) fn resume_content_refresh_after_runtime_retirement(&mut self) {
        let EditorProjectContentView::Stale {
            failure: EditorProjectContentRefreshFailure::RuntimeRetirementRequired,
            ..
        } = self.project_content
        else {
            return;
        };
        let Some(lane) = self.trusted_compiled() else {
            return;
        };
        if !matches!(&lane.runtime_owner, EditorRuntimeOwnerState::Empty)
            || lane.runtime_host.running_content_revision().is_some()
            || lane.runtime_host.has_cleanup_owner()
            || self.pending_content_refresh.is_some()
        {
            return;
        }
        let Some(request) = self.failed_content_refresh.take() else {
            return;
        };
        let Some(current_generation) = self.project_content.current_generation() else {
            self.failed_content_refresh = Some(request);
            return;
        };
        self.project_content = refresh_pending_view(current_generation, &request);
        self.pending_content_refresh = Some(request);
    }

    pub(super) fn advance_project_content_refresh(&mut self) {
        // A replacement runtime would retire the file-custody participant before the
        // current session can recover its Host and acknowledge the admitted Save.
        if self.pending_persistence.is_some() {
            return;
        }
        let Some(request) = self.pending_content_refresh.take() else {
            return;
        };
        let (successor, current_revision, preview_registry) = {
            let Some(lane) = self.trusted_compiled() else {
                self.failed_content_refresh = Some(request);
                return;
            };
            let runtime_agreement = match prepare_runtime_schema_agreement(
                &lane.content_loader.project_root(),
                &lane.content_candidate,
                &lane.plan,
            ) {
                Ok(agreement) => agreement,
                Err(error) => {
                    self.reject_project_content_refresh(
                        request,
                        self.classify_refresh_error(&error),
                    );
                    let _ = self.diagnostics.extend(error.diagnostics().clone());
                    return;
                }
            };
            let successor = match lane
                .content_loader
                .load_with_expected_prefab_digest_and_runtime_agreement(
                    &lane.content_candidate,
                    &lane.plan,
                    request.prefab.clone(),
                    request.expected_digest,
                    runtime_agreement,
                ) {
                Ok(successor) => successor,
                Err(error) => {
                    self.reject_project_content_refresh(
                        request,
                        self.classify_refresh_error(&error),
                    );
                    let _ = self.diagnostics.extend(error.diagnostics().clone());
                    return;
                }
            };
            (
                successor,
                lane.snapshot.revision(),
                lane.plan.schema_validation().snapshot().clone(),
            )
        };

        if successor.revision() == current_revision {
            let Some(generation) = self.project_content.current_generation() else {
                self.reject_project_content_refresh(
                    request,
                    EditorProjectContentRefreshFailure::InvalidContent,
                );
                return;
            };
            self.project_content = EditorProjectContentView::Current { generation };
            self.failed_content_refresh = None;
            return;
        }

        let replacement_authoring_runtime = match self
            .authoring_runtime
            .rebuild_for_project(successor.clone(), &preview_registry)
        {
            Ok(runtime) => runtime,
            Err(report) => {
                self.reject_project_content_refresh(
                    request,
                    EditorProjectContentRefreshFailure::PreviewRejected,
                );
                let _ = self.diagnostics.extend(report);
                return;
            }
        };
        #[cfg(feature = "runtime-ui")]
        let replacement_authoring_images = match self
            .authoring_images
            .prepare_replacement_successor(&replacement_authoring_runtime)
        {
            Ok(successor) => successor,
            Err(_) => {
                self.reject_project_content_refresh(
                    request,
                    EditorProjectContentRefreshFailure::PreviewRejected,
                );
                let _ = self.diagnostics.extend(single_error(
                    "project.editor.content-refresh-image-catalog-rejected",
                    "the saved project content could not publish a complete authoring image catalog",
                ));
                return;
            }
        };
        #[cfg(feature = "presentation-ui")]
        let replacement_ui_preview = match super::ui_preview::EditorUiPreviewSession::new() {
            Ok(preview) => preview,
            Err(_) => {
                self.reject_project_content_refresh(
                    request,
                    EditorProjectContentRefreshFailure::PreviewRejected,
                );
                let _ = self.diagnostics.extend(single_error(
                    "project.editor.content-refresh-ui-preview-rejected",
                    "the saved project content could not replace the Editor UI preview owner",
                ));
                return;
            }
        };

        let generation = EditorProjectContentGeneration::new(self.next_project_content_generation);
        let Some(next_generation) = self.next_project_content_generation.checked_add(1) else {
            self.reject_project_content_refresh(
                request,
                EditorProjectContentRefreshFailure::CapacityExceeded,
            );
            let _ = self.diagnostics.extend(single_error(
                "project.editor.content-generation-exhausted",
                "the Editor project-content publication identity is exhausted",
            ));
            return;
        };
        #[cfg(feature = "runtime-ui")]
        if self
            .authoring_images
            .publish_successor(replacement_authoring_images)
            .is_err()
        {
            self.reject_project_content_refresh(
                request,
                EditorProjectContentRefreshFailure::PreviewRejected,
            );
            return;
        }
        self.trusted_compiled_mut()
            .expect("content refresh begins from the trusted compiled lane")
            .snapshot = successor;
        self.authoring_runtime = replacement_authoring_runtime;
        #[cfg(feature = "presentation-ui")]
        {
            self.ui_preview = replacement_ui_preview;
        }
        #[cfg(feature = "runtime-ui")]
        {
            self.ui_authoring_frame = None;
        }
        self.next_project_content_generation = next_generation;
        self.project_content = EditorProjectContentView::Current { generation };
        self.failed_content_refresh = None;
    }

    fn reject_project_content_refresh(
        &mut self,
        request: PendingProjectContentRefresh,
        failure: EditorProjectContentRefreshFailure,
    ) {
        let Some(current_generation) = self.project_content.current_generation() else {
            self.failed_content_refresh = Some(request);
            return;
        };
        self.project_content = EditorProjectContentView::Stale {
            current_generation,
            document: request.document,
            revision: request.revision,
            saved_digest: request.saved_digest,
            failure,
        };
        self.failed_content_refresh = Some(request);
    }

    fn classify_refresh_error(
        &self,
        error: &ProjectContentError,
    ) -> EditorProjectContentRefreshFailure {
        match error.kind() {
            ProjectContentErrorKind::HostAuthorityRejected => {
                EditorProjectContentRefreshFailure::SavedSourceChanged
            }
            ProjectContentErrorKind::HostIo
            | ProjectContentErrorKind::HostAuthorityUnsupported
            | ProjectContentErrorKind::HostAuthorityUnproven => {
                EditorProjectContentRefreshFailure::HostUnavailable
            }
            ProjectContentErrorKind::GenerationCapacityExceeded => {
                let Some(lane) = self.trusted_compiled() else {
                    return EditorProjectContentRefreshFailure::CapacityExceeded;
                };
                let runtime_retirement_can_release_a_generation = lane
                    .runtime_host
                    .running_content_revision()
                    .is_some_and(|revision| revision != lane.snapshot.revision())
                    || lane.runtime_host.has_cleanup_owner();
                if runtime_retirement_can_release_a_generation {
                    EditorProjectContentRefreshFailure::RuntimeRetirementRequired
                } else {
                    EditorProjectContentRefreshFailure::CapacityExceeded
                }
            }
            ProjectContentErrorKind::BudgetExceeded | ProjectContentErrorKind::AllocationFailed => {
                EditorProjectContentRefreshFailure::CapacityExceeded
            }
            ProjectContentErrorKind::InvalidLimits
            | ProjectContentErrorKind::ProjectLineageMismatch
            | ProjectContentErrorKind::ProjectRootUnbound
            | ProjectContentErrorKind::ProjectRootMismatch
            | ProjectContentErrorKind::MissingStartupScene
            | ProjectContentErrorKind::InvalidLogicalPath
            | ProjectContentErrorKind::ClosurePathCollision
            | ProjectContentErrorKind::SceneFormat
            | ProjectContentErrorKind::ScenePublication
            | ProjectContentErrorKind::PrefabExpansion
            | ProjectContentErrorKind::AssetReference
            | ProjectContentErrorKind::AssetMeta
            | ProjectContentErrorKind::AssetMetaMismatch
            | ProjectContentErrorKind::UnsupportedAssetKind
            | ProjectContentErrorKind::ImageImport
            | ProjectContentErrorKind::UnsupportedHierarchySemantics
            | ProjectContentErrorKind::SchemaMismatch => {
                EditorProjectContentRefreshFailure::InvalidContent
            }
        }
    }
}

fn refresh_pending_view(
    current_generation: EditorProjectContentGeneration,
    request: &PendingProjectContentRefresh,
) -> EditorProjectContentView {
    EditorProjectContentView::RefreshPending {
        current_generation,
        document: request.document,
        revision: request.revision,
        saved_digest: request.saved_digest,
    }
}
