use nara_asset::AssetPath;
use nara_fs::ContentDigest;
use nara_tooling::{
    EditorDocumentId, EditorProjectContentAction, EditorProjectContentRefreshFailure,
    EditorProjectContentRefreshRejection, EditorProjectContentRefreshRequestResult,
    EditorProjectContentView,
};

use crate::project_host::persistence::editor_digest;
use crate::project_host::runtime_agreement::prepare_runtime_schema_agreement;
use crate::project_host::{ProjectContentError, ProjectContentErrorKind, ProjectContentMemberKind};

use super::{
    EditorPersistenceReceipt, EditorProjectContentGeneration, EditorProjectSession,
    EditorRuntimeOwnerState, single_error,
};

#[derive(Clone, Copy)]
pub(super) struct PrefabContentRefreshObligation {
    document: EditorDocumentId,
    revision: nara_scene::SceneAuthoringRevision,
    expected_digest: ContentDigest,
}

impl EditorProjectSession {
    pub(super) fn advance_authoring_retirement(&mut self) {
        if self.authoring_retirements.poll_for_new_cleanup_failure() {
            let _ = self.diagnostics.extend(single_error(
                "project.editor.authoring-retirement-failed",
                "An authoring Runtime returned with a terminal cleanup failure",
            ));
        }
    }

    fn retain_authoring_runtime(
        &mut self,
        runtime: super::authoring_runtime::EditorAuthoringRuntime,
    ) {
        self.authoring_retirements.retain(runtime);
        self.advance_authoring_retirement();
    }

    pub(super) fn schedule_prefab_content_refresh(&mut self, evidence: EditorPersistenceReceipt) {
        let document = evidence.document();
        let path = self
            .prefab_source_key(document)
            .expect("a Runtime Prefab Host publishes inside the configured Prefab domain");
        let saved_digest = evidence
            .published_digest()
            .expect("an accepted Prefab save carries verified published bytes");
        self.queue_prefab_content_refresh(
            document,
            evidence.revision(),
            path,
            ContentDigest::from_parts(saved_digest.encoded_bytes(), saved_digest.hash()),
        );
    }

    pub(super) fn schedule_reopened_prefab_content_refresh(
        &mut self,
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        content_digest: ContentDigest,
    ) {
        let path = self
            .prefab_source_key(document)
            .expect("a Runtime Prefab Host reopens inside the configured Prefab domain");
        self.queue_prefab_content_refresh(document, revision, path, content_digest);
    }

    fn queue_prefab_content_refresh(
        &mut self,
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        prefab: AssetPath,
        expected_digest: ContentDigest,
    ) {
        let Some(current_generation) = self.project_content.current_generation() else {
            return;
        };
        if self.current_snapshot_has_prefab_digest(&prefab, expected_digest) {
            self.prefab_content_refresh.remove(&prefab);
        } else {
            self.prefab_content_refresh.insert(
                prefab,
                PrefabContentRefreshObligation {
                    document,
                    revision,
                    expected_digest,
                },
            );
        }
        self.project_content = self.prefab_content_refresh.values().next().map_or(
            EditorProjectContentView::Current {
                generation: current_generation,
            },
            |request| refresh_pending_view(current_generation, request),
        );
    }

    fn current_snapshot_has_prefab_digest(
        &self,
        prefab: &AssetPath,
        digest: ContentDigest,
    ) -> bool {
        let Some(lane) = self.trusted_compiled() else {
            return false;
        };
        let path = lane.snapshot.prefab_resolution().scope().resolve(prefab);
        lane.snapshot
            .content_closure()
            .members()
            .iter()
            .any(|member| {
                member.kind() == ProjectContentMemberKind::Prefab
                    && member.path().as_str() == path.as_str()
                    && member.digest() == digest
            })
    }

    pub(super) fn request_project_content_refresh(
        &mut self,
        action: EditorProjectContentAction,
    ) -> EditorProjectContentRefreshRequestResult {
        if !self.accepts_new_work() || self.source_tools.reclamation_fenced() {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RetryNotAvailable,
            );
        }
        if self.trusted_compiled().is_none() {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RetryNotAvailable,
            );
        }
        match action {
            EditorProjectContentAction::RetryRefresh => {}
        }
        if matches!(
            self.project_content,
            EditorProjectContentView::RefreshPending { .. }
        ) {
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
        let Some(request) = self.prefab_content_refresh.values().next() else {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RetryNotAvailable,
            );
        };
        let Some(current_generation) = self.project_content.current_generation() else {
            return EditorProjectContentRefreshRequestResult::Rejected(
                EditorProjectContentRefreshRejection::RetryNotAvailable,
            );
        };
        self.project_content = refresh_pending_view(current_generation, request);
        EditorProjectContentRefreshRequestResult::Accepted
    }

    pub(super) fn resume_content_refresh_after_runtime_retirement(&mut self) {
        if !self.accepts_new_work() {
            return;
        }
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
        {
            return;
        }
        let Some(request) = self.prefab_content_refresh.values().next() else {
            return;
        };
        let Some(current_generation) = self.project_content.current_generation() else {
            return;
        };
        self.project_content = refresh_pending_view(current_generation, request);
    }

    pub(super) fn advance_project_content_refresh(&mut self) {
        self.advance_authoring_retirement();
        // Transfer the lifetime Host owner only when no operation still borrows its runtime.
        if !self.accepts_new_work()
            || self.authoring_retirements.is_pending()
            || self.persistence.has_pending_work()
            || !matches!(
                self.project_content,
                EditorProjectContentView::RefreshPending { .. }
            )
        {
            return;
        }
        let Some(request) = self.prefab_content_refresh.values().next().copied() else {
            return;
        };
        let (successor, current_revision, preview_registry) = {
            let Some(lane) = self.trusted_compiled() else {
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
                .load_with_expected_prefab_digests_and_runtime_agreement(
                    &lane.content_candidate,
                    &lane.plan,
                    self.prefab_content_refresh
                        .iter()
                        .map(|(path, obligation)| (path, obligation.expected_digest)),
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
            self.prefab_content_refresh.clear();
            return;
        }

        let replacement_authoring_runtime = match self
            .authoring_runtime
            .rebuild_for_project(successor.clone(), &preview_registry)
        {
            Ok(runtime) => runtime,
            Err(failure) => {
                self.authoring_retirements.retain(failure.retirement);
                self.advance_authoring_retirement();
                self.reject_project_content_refresh(
                    request,
                    EditorProjectContentRefreshFailure::PreviewRejected,
                );
                let _ = self.diagnostics.extend(failure.diagnostics);
                return;
            }
        };
        #[cfg(test)]
        if let Some(hook) = self.next_authoring_refresh_hook.take() {
            hook(&replacement_authoring_runtime);
        }
        #[cfg(feature = "runtime-ui")]
        let replacement_authoring_images = match self
            .authoring_images
            .prepare_replacement_successor(&replacement_authoring_runtime)
        {
            Ok(successor) => successor,
            Err(_) => {
                self.retain_authoring_runtime(replacement_authoring_runtime);
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
                self.retain_authoring_runtime(replacement_authoring_runtime);
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
            self.retain_authoring_runtime(replacement_authoring_runtime);
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
            .validate_successor(&replacement_authoring_images)
            .is_err()
        {
            self.retain_authoring_runtime(replacement_authoring_runtime);
            self.reject_project_content_refresh(
                request,
                EditorProjectContentRefreshFailure::PreviewRejected,
            );
            return;
        }
        if !self
            .authoring_runtime
            .host_custody()
            .transfer_to(replacement_authoring_runtime.host_custody())
        {
            self.retain_authoring_runtime(replacement_authoring_runtime);
            self.reject_project_content_refresh(
                request,
                EditorProjectContentRefreshFailure::PreviewRejected,
            );
            return;
        }
        #[cfg(feature = "runtime-ui")]
        self.authoring_images
            .publish_successor(replacement_authoring_images)
            .expect("exclusive refresh publication retains the validated image predecessor");
        self.trusted_compiled_mut()
            .expect("content refresh begins from the trusted compiled lane")
            .snapshot = successor;
        let predecessor =
            std::mem::replace(&mut self.authoring_runtime, replacement_authoring_runtime);
        self.retain_authoring_runtime(predecessor);
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
        self.prefab_content_refresh.clear();
    }

    fn reject_project_content_refresh(
        &mut self,
        request: PrefabContentRefreshObligation,
        failure: EditorProjectContentRefreshFailure,
    ) {
        let Some(current_generation) = self.project_content.current_generation() else {
            return;
        };
        self.project_content = EditorProjectContentView::Stale {
            current_generation,
            document: request.document,
            revision: request.revision,
            saved_digest: editor_digest(request.expected_digest),
            failure,
        };
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
            | ProjectContentErrorKind::MissingSelectedScene
            | ProjectContentErrorKind::InvalidLogicalPath
            | ProjectContentErrorKind::ClosurePathCollision
            | ProjectContentErrorKind::SceneFormat
            | ProjectContentErrorKind::ScenePublication
            | ProjectContentErrorKind::PrefabExpansion
            | ProjectContentErrorKind::AssetReference
            | ProjectContentErrorKind::AssetMeta
            | ProjectContentErrorKind::AssetDomain
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
    request: &PrefabContentRefreshObligation,
) -> EditorProjectContentView {
    EditorProjectContentView::RefreshPending {
        current_generation,
        document: request.document,
        revision: request.revision,
        saved_digest: editor_digest(request.expected_digest),
    }
}
