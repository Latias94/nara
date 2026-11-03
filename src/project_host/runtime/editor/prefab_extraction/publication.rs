//! Reviewed extraction confirmation uses original publication custody and original destinations.

use std::mem::size_of;

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectSourceAdmissionError, PrefabExtractionPersistenceDestination,
    PrefabExtractionSessionPublicationError, PrefabExtractionSessionPublicationPhase,
};

impl EditorProjectSession {
    pub(super) fn advance_prefab_extraction_publication(&mut self) {
        use EditorPrefabExtractionPhase as P;
        if let Some(mut pending) = self.prefab_extraction.publication_work.take() {
            let Some(result) = pending.poll() else {
                self.prefab_extraction.publication_work = Some(pending);
                return;
            };
            match result {
                Ok(mut publication) => {
                    self.prefab_extraction.view.cancellation_allowed =
                        publication.cancellation_allowed();
                    if self.prefab_extraction.cancelled {
                        publication.request_cancellation();
                    }
                    self.prefab_extraction.publication = Some(publication);
                }
                Err(_) => {
                    // No actual owner returned. Neither Close nor an idle Task proves that
                    // the old manifest is still current; retain admission for recovery.
                    self.prefab_extraction.view.phase = P::RequiresReconcile;
                    self.prefab_extraction.view.work_pending = false;
                    self.prefab_extraction.view.cancellation_allowed = false;
                    self.prefab_extraction.view.retry_allowed = false;
                    self.prefab_extraction.view.refusal = Some(EditorPrefabExtractionError::Task);
                    return;
                }
            }
        }
        if self.prefab_extraction.view.phase == P::RequiresReconcile
            && !self.prefab_extraction.cancelled
            && !self
                .prefab_extraction
                .publication
                .as_ref()
                .is_some_and(PrefabExtractionSessionPublication::retry_requested)
        {
            return;
        }
        if self.supervised_play_retirement_pending()
            || self
                .source_tools
                .physical_work_pending(self.authoring_runtime.host_custody())
        {
            self.prefab_extraction.view.phase = P::Retiring;
            return;
        }
        if self.source_tools.process_cleanup_failed() {
            self.prefab_extraction.view.refusal =
                Some(EditorPrefabExtractionError::ExecutionCleanup);
            self.prefab_extraction.view.work_pending = false;
            if !self.prefab_extraction.cancelled {
                return;
            }
        }
        if self.prefab_extraction.cancelled {
            let Some(publication) = self.prefab_extraction.publication.as_ref() else {
                return;
            };
            if publication.settle_cancellation() {
                if self.finish_supervised_play_retirement().is_err() {
                    return;
                }
                self.prefab_extraction
                    .finish(EditorPrefabExtractionOutcome::Cancelled);
                return;
            }
        }
        let Some(publication) = self.prefab_extraction.publication.take() else {
            return;
        };
        if publication.failure().is_some() && !publication.retry_requested() {
            self.observe_prefab_extraction_publication_failure(&publication);
            self.prefab_extraction.publication = Some(publication);
            return;
        }
        if publication.phase() == PrefabExtractionSessionPublicationPhase::Ready
            && !publication.retry_requested()
        {
            self.prefab_extraction.view.phase = P::Adopting;
            self.install_prefab_extraction(publication);
            return;
        }
        let phase = match publication.phase() {
            PrefabExtractionSessionPublicationPhase::Confirming => P::Confirming,
            PrefabExtractionSessionPublicationPhase::Recovery => P::PublishingRecovery,
            PrefabExtractionSessionPublicationPhase::Pending => P::PublishingPending,
            PrefabExtractionSessionPublicationPhase::Sources => P::PublishingSources,
            PrefabExtractionSessionPublicationPhase::PreparingReady => P::PreparingReady,
            PrefabExtractionSessionPublicationPhase::Ready => P::Adopting,
        };
        let attempts_pending = publication.pending_attempt_is_next();
        let operation = self.prefab_extraction.view.operation.unwrap();
        match PublicationWork::start(
            self.authoring_runtime.persistence_tasks(),
            nara_tasks::TaskSpawnRequest::new(operation.0, DOMAIN),
            publication,
        ) {
            Ok(pending) => {
                self.prefab_extraction.view.retry_allowed = false;
                if attempts_pending {
                    // Install this fence before the native publisher can replace Pending.
                    self.prefab_extraction.view.cancellation_allowed = false;
                }
                self.prefab_extraction.publication_work = Some(pending);
                self.prefab_extraction.view.phase = phase;
                self.prefab_extraction.view.work_pending = true;
            }
            Err(mut failure) => {
                failure.publication.defer_retry();
                self.observe_prefab_extraction_publication_failure(&failure.publication);
                if !failure.publication.host_work_exhausted()
                    && !failure.publication.open_handles_exceeded()
                {
                    self.prefab_extraction.view.refusal = Some(failure.refusal);
                }
                self.prefab_extraction.publication = Some(failure.publication);
            }
        }
    }

    fn observe_prefab_extraction_publication_failure(
        &mut self,
        publication: &PrefabExtractionSessionPublication,
    ) {
        self.prefab_extraction.view.phase = EditorPrefabExtractionPhase::RequiresReconcile;
        self.prefab_extraction.view.work_pending = false;
        self.prefab_extraction.view.retry_allowed = publication.can_retry();
        self.prefab_extraction.view.cancellation_allowed = publication.cancellation_allowed();
        let (reason, code, summary) = if publication.host_work_exhausted() {
            (
                EditorPrefabExtractionError::WorkLimit,
                "project.editor.prefab-extraction-work-limit",
                "The complete extraction exhausted its selected cumulative Host work allowance",
            )
        } else if publication.open_handles_exceeded() {
            (
                EditorPrefabExtractionError::OpenHandleLimit,
                "project.editor.prefab-extraction-handle-limit",
                "The complete extraction exceeds its selected native-handle ceiling",
            )
        } else if publication
            .failure()
            .is_some_and(|failure| failure.source_limit_exceeded())
        {
            (
                EditorPrefabExtractionError::SourceLimit,
                "project.editor.prefab-extraction-source-limit",
                "The original extraction Source allowance refused further publication validation",
            )
        } else if let Some(failure) = publication.failure() {
            let (code, summary) = failure.diagnostic();
            (EditorPrefabExtractionError::Publication, code, summary)
        } else {
            (
                EditorPrefabExtractionError::Publication,
                "project.editor.prefab-extraction-publication-refused",
                "The actual extraction publication requires reconciliation under its original write fence",
            )
        };
        self.prefab_extraction.view.refusal = Some(reason);
        self.prefab_extraction.diagnostics = Some(
            crate::project_host::runtime_diagnostics::single_error(code, summary),
        );
    }

    fn install_prefab_extraction(&mut self, mut publication: PrefabExtractionSessionPublication) {
        let result = bind_and_publish(
            &mut self.workspace,
            &mut self.persistence_authority,
            &mut self.scene_lanes,
            &mut self.prefab_lanes,
            &mut self.execution,
            &mut publication,
        );
        if result.is_err() {
            self.observe_prefab_extraction_publication_failure(&publication);
            self.prefab_extraction.publication = Some(publication);
            return;
        }
        self.authoring_prefab_source_observations = BTreeMap::new();
        self.prefab_content_refresh = BTreeMap::new();
        self.finish_supervised_play_retirement().expect(
            "the acknowledged extraction custodian remains registered through installation",
        );
        #[cfg(feature = "presentation-2d")]
        {
            self.authoring_runtime.invalidate_preview_content();
            self.scene_capture.invalidate();
        }
        #[cfg(feature = "runtime-ui")]
        {
            self.ui_authoring_frame = None;
            self.ui_authoring_projection_cache = None;
        }
        #[cfg(feature = "presentation-ui")]
        self.ui_preview.retire();
        // The old Project and finished resource custody survive all final destination moves.
        drop(publication);
        self.prefab_extraction
            .finish(EditorPrefabExtractionOutcome::Adopted);
    }
}

enum HostGuard<'a> {
    Scene(persistence::RetainedHostGuard<'a, ScenePersistenceHost>),
    Prefab(persistence::RetainedHostGuard<'a, PrefabPersistenceHost>),
}

fn bind_and_publish(
    workspace: &mut EditorWorkspace,
    authority: &EditorPersistenceAuthority,
    scenes: &mut BTreeMap<EditorDocumentId, EditorScenePersistenceLane>,
    prefabs: &mut BTreeMap<EditorDocumentId, EditorPrefabPersistenceLane>,
    execution: &mut EditorExecutionLane,
    publication: &mut PrefabExtractionSessionPublication,
) -> Result<(), ()> {
    // These vectors are temporary binding storage, independent of the preprepared continuing
    // source facts. Admit each path and slot before allocating or retaining the original locks.
    let count = publication.installation_documents().len();
    let bytes = count
        .checked_mul(
            size_of::<HostGuard<'_>>()
                + size_of::<(EditorDocumentId, String)>()
                + size_of::<PrefabExtractionPersistenceDestination<'_>>(),
        )
        .and_then(|bytes| {
            publication
                .installation_documents()
                .try_fold(bytes, |bytes, (_, source)| {
                    bytes.checked_add(source.relative_path().len())
                })
        });
    let Some(bytes) = bytes else {
        publication.record_failure(AuthoringProjectSourceAdmissionError::AccountingOverflow.into());
        return Err(());
    };
    if let Err(error) = publication.admit_installation_storage(bytes) {
        publication.record_failure(error);
        return Err(());
    }
    let result = bind_and_publish_admitted(
        workspace,
        authority,
        scenes,
        prefabs,
        execution,
        publication,
    );
    // Every path/destination/Host guard was physically dropped before this refund, including
    // refusal. Successful continuing facts have moved to the exact original persistence lanes.
    publication.release_installation_storage(bytes);
    result
}

fn bind_and_publish_admitted(
    workspace: &mut EditorWorkspace,
    authority: &EditorPersistenceAuthority,
    scenes: &mut BTreeMap<EditorDocumentId, EditorScenePersistenceLane>,
    prefabs: &mut BTreeMap<EditorDocumentId, EditorPrefabPersistenceLane>,
    execution: &mut EditorExecutionLane,
    publication: &mut PrefabExtractionSessionPublication,
) -> Result<(), ()> {
    let installed = {
        let count = publication.installation_documents().len();
        let originals = (|| -> Result<_, PrefabExtractionSessionPublicationError> {
            let mut paths = Vec::with_capacity(count);
            let mut guards = Vec::with_capacity(count);
            for (document, original) in publication.installation_documents() {
                let guard = if let Some(lane) = scenes.get(&document) {
                    if lane.authoring_source.as_ref() != Some(original)
                        || lane.source_key != *original.source_key()
                    {
                        return Err(
                            nara_tooling::EditorPersistenceRejection::SourceBindingMismatch.into(),
                        );
                    }
                    lane.host.try_bind().map(HostGuard::Scene)
                } else if let Some(lane) = prefabs.get(&document) {
                    if lane.authoring_source.as_ref() != Some(original)
                        || lane.source_key != *original.source_key()
                    {
                        return Err(
                            nara_tooling::EditorPersistenceRejection::SourceBindingMismatch.into(),
                        );
                    }
                    lane.host.try_bind().map(HostGuard::Prefab)
                } else {
                    return Err(nara_tooling::EditorPersistenceRejection::MissingDocument.into());
                };
                let guard =
                    guard.ok_or(nara_tooling::EditorPersistenceRejection::LockUnavailable)?;
                paths.push((document, original.relative_path().to_owned()));
                guards.push(guard);
            }
            Ok((paths, guards))
        })();
        let (paths, mut guards) = match originals {
            Ok(originals) => originals,
            Err(error) => {
                publication.record_failure(error);
                return Err(());
            }
        };
        let mut destinations = Vec::with_capacity(count);
        for ((document, path), guard) in paths.iter().zip(guards.iter_mut()) {
            destinations.push(match guard {
                HostGuard::Scene(guard) => PrefabExtractionPersistenceDestination::Scene {
                    document: *document,
                    project_path: path,
                    host: guard.host_mut(),
                },
                HostGuard::Prefab(guard) => PrefabExtractionPersistenceDestination::Prefab {
                    document: *document,
                    project_path: path,
                    host: guard.host_mut(),
                },
            });
        }
        publication.install(authority, workspace, &mut destinations)
    };
    let installed = installed?;
    // Workspace, leaf, lane and execution destinations were held across Ready. Everything
    // below consumes already prepared state; no new lookup, file read or admission can fail.
    for (document, source) in installed.sources {
        if let Some(lane) = scenes.get_mut(&document) {
            lane.authoring_source = Some(source);
        } else {
            prefabs
                .get_mut(&document)
                .expect("the all-bound original Prefab lane remains present")
                .authoring_source = Some(source);
        }
    }
    *execution = EditorExecutionLane::Authoring(installed.session);
    drop(installed.receipts);
    Ok(())
}
