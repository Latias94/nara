use nara_asset::AssetPath;
use nara_fs::ContentDigest;
use nara_tooling::{
    EditorDocumentDigest, EditorDocumentId, EditorProjectContentAction,
    EditorProjectContentRefreshFailure, EditorProjectContentRefreshRejection,
    EditorProjectContentRefreshRequestResult, EditorProjectContentView,
};

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
        if self.current_snapshot_has_prefab_digest(&prefab, expected_digest) {
            self.pending_content_refresh = None;
            self.failed_content_refresh = None;
            self.project_content = EditorProjectContentView::Current {
                generation: self.project_content.current_generation(),
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
        self.project_content =
            refresh_pending_view(self.project_content.current_generation(), &request);
        self.pending_content_refresh = Some(request);
        self.failed_content_refresh = None;
    }

    fn current_snapshot_has_prefab_digest(
        &self,
        prefab: &AssetPath,
        digest: ContentDigest,
    ) -> bool {
        let path = format!(
            "{}/{}",
            self.content_candidate.settings().paths.prefabs.as_str(),
            prefab.as_str()
        );
        self.snapshot
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
        self.project_content =
            refresh_pending_view(self.project_content.current_generation(), &request);
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
        if !matches!(self.runtime_owner, EditorRuntimeOwnerState::Empty)
            || self.runtime_host.running_content_revision().is_some()
            || self.runtime_host.has_cleanup_owner()
            || self.pending_content_refresh.is_some()
        {
            return;
        }
        let Some(request) = self.failed_content_refresh.take() else {
            return;
        };
        self.project_content =
            refresh_pending_view(self.project_content.current_generation(), &request);
        self.pending_content_refresh = Some(request);
    }

    pub(super) fn advance_project_content_refresh(&mut self) {
        let Some(request) = self.pending_content_refresh.take() else {
            return;
        };
        let successor = match self.content_loader.load_with_expected_prefab_digest(
            &self.content_candidate,
            &self.plan,
            request.prefab.clone(),
            request.expected_digest,
        ) {
            Ok(successor) => successor,
            Err(error) => {
                self.reject_project_content_refresh(request, self.classify_refresh_error(&error));
                let _ = self.diagnostics.extend(error.diagnostics().clone());
                return;
            }
        };

        if successor.revision() == self.snapshot.revision() {
            self.project_content = EditorProjectContentView::Current {
                generation: self.project_content.current_generation(),
            };
            self.failed_content_refresh = None;
            return;
        }

        #[cfg(feature = "presentation-2d")]
        let replacement_preview = match self
            .preview
            .rebuild_for_project(successor.clone(), self.plan.schema_validation().snapshot())
        {
            Ok(preview) => preview,
            Err(report) => {
                self.reject_project_content_refresh(
                    request,
                    EditorProjectContentRefreshFailure::PreviewRejected,
                );
                let _ = self.diagnostics.extend(report);
                return;
            }
        };
        #[cfg(feature = "presentation-ui")]
        let replacement_ui_preview =
            match super::ui_preview::EditorUiPreviewSession::new(&successor) {
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

        self.snapshot = successor;
        #[cfg(feature = "presentation-2d")]
        {
            self.preview = replacement_preview;
        }
        #[cfg(feature = "presentation-ui")]
        {
            self.ui_preview = replacement_ui_preview;
        }
        #[cfg(feature = "runtime-ui")]
        {
            self.ui_authoring_frame = None;
            self.ui_authoring_projection_cache = None;
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
        self.project_content = EditorProjectContentView::Stale {
            current_generation: self.project_content.current_generation(),
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
                let runtime_retirement_can_release_a_generation = self
                    .runtime_host
                    .running_content_revision()
                    .is_some_and(|revision| revision != self.snapshot.revision())
                    || self.runtime_host.has_cleanup_owner();
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
            | ProjectContentErrorKind::UnsupportedStableAssetReference
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
