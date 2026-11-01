//! Session confirmation, physical retirement and the final exclusive owner turn.

use super::*;
use crate::project_host::{
    authoring::{AuthoredSourceUpgradePublicationFailure, SourceUpgradeProjectOutcome},
    persistence::{
        PersistenceAuthoredUpgradeCommit, PersistenceAuthoredUpgradeValidationCost, editor_digest,
    },
};
use nara_tooling::EditorAuthoredUpgradeSavedSource;

impl EditorProjectSession {
    pub(super) fn advance_authored_schema_upgrade_publication(&mut self) {
        use EditorAuthoredSchemaUpgradePhase as P;
        if let Some(mut pending) = self.authored_schema_upgrade.pending.take() {
            let Some(result) = pending.poll() else {
                self.authored_schema_upgrade.pending = Some(pending);
                return;
            };
            match result {
                Ok(Returned::Published(mut publication)) => {
                    self.authored_schema_upgrade.view.cancellation_allowed =
                        publication.cancellation_allowed();
                    if self.authored_schema_upgrade.cancelled {
                        publication.request_cancellation();
                    }
                    self.authored_schema_upgrade.publication = Some(publication);
                }
                _ => {
                    // A lost durable publisher return is not proof of unchanged disk. Keep
                    // the fence, including after native Close, for explicit recovery.
                    self.authored_schema_upgrade.view.phase = P::RequiresReconcile;
                    self.authored_schema_upgrade.view.work_pending = false;
                    self.authored_schema_upgrade.view.cancellation_allowed = false;
                    self.authored_schema_upgrade.view.retry_allowed = false;
                    self.authored_schema_upgrade.view.refusal =
                        Some(EditorAuthoredSchemaUpgradeError::Task);
                    return;
                }
            }
        }
        if self.authored_schema_upgrade.view.phase == P::RequiresReconcile
            && !self.authored_schema_upgrade.cancelled
            && !self
                .authored_schema_upgrade
                .publication
                .as_ref()
                .is_some_and(AuthoredSourceUpgradePublication::retry_requested)
        {
            return;
        }
        if self.supervised_play_retirement_pending()
            || self
                .source_tools
                .physical_work_pending(self.authoring_runtime.host_custody())
        {
            self.authored_schema_upgrade.view.phase = P::Retiring;
            return;
        }
        if self.source_tools.process_cleanup_failed() {
            self.authored_schema_upgrade.view.refusal =
                Some(EditorAuthoredSchemaUpgradeError::ExecutionCleanup);
            self.authored_schema_upgrade.view.work_pending = false;
            // Retirement failure cannot authorize destructive publication. A pre-Pending
            // cancellation can still release the original sources once custody has returned.
            if !self.authored_schema_upgrade.cancelled {
                return;
            }
        }
        if self.authored_schema_upgrade.cancelled {
            let Some(publication) = self.authored_schema_upgrade.publication.as_ref() else {
                return;
            };
            if publication.settle_cancellation() {
                if self.finish_supervised_play_retirement().is_err() {
                    return;
                }
                self.authored_schema_upgrade
                    .finish(EditorAuthoredSchemaUpgradeOutcome::Cancelled);
                return;
            }
            if !publication.retry_requested() {
                self.authored_schema_upgrade.view.phase = P::RequiresReconcile;
                self.authored_schema_upgrade.view.work_pending = false;
                let publication = self.authored_schema_upgrade.publication.take().unwrap();
                self.observe_authored_schema_upgrade_publication_failure(&publication);
                self.authored_schema_upgrade.publication = Some(publication);
                return;
            }
        }
        let Some(publication) = self.authored_schema_upgrade.publication.take() else {
            return;
        };
        if publication.failure().is_some() && !publication.retry_requested() {
            self.authored_schema_upgrade.view.phase = P::RequiresReconcile;
            self.authored_schema_upgrade.view.work_pending = false;
            self.observe_authored_schema_upgrade_publication_failure(&publication);
            self.authored_schema_upgrade.publication = Some(publication);
            return;
        }
        if publication.phase() == AuthoredSourceUpgradePublicationPhase::Ready
            && !publication.retry_requested()
        {
            self.authored_schema_upgrade.view.phase = P::Adopting;
            self.install_authored_schema_upgrade(publication);
            return;
        }
        let phase = match publication.phase() {
            AuthoredSourceUpgradePublicationPhase::Confirming => P::Confirming,
            AuthoredSourceUpgradePublicationPhase::Recovery => P::PublishingRecovery,
            AuthoredSourceUpgradePublicationPhase::Pending => P::PublishingPending,
            AuthoredSourceUpgradePublicationPhase::Sources => P::PublishingSources,
            AuthoredSourceUpgradePublicationPhase::PreparingReady => P::PreparingReady,
            AuthoredSourceUpgradePublicationPhase::Ready => P::Adopting,
        };
        let attempts_pending = publication.pending_attempt_is_next();
        let operation = self.authored_schema_upgrade.view.operation.unwrap();
        match PublicationWork::start(
            self.authoring_runtime.persistence_tasks(),
            TaskSpawnRequest::new(operation.0, DOMAIN),
            publication,
        ) {
            Ok(pending) => {
                self.authored_schema_upgrade.view.retry_allowed = false;
                // The exclusive Session turn installs this fence before any caller can cancel.
                if attempts_pending {
                    self.authored_schema_upgrade.view.cancellation_allowed = false;
                }
                self.authored_schema_upgrade.pending = Some(Work::Publish(pending));
                self.authored_schema_upgrade.view.phase = phase;
                self.authored_schema_upgrade.view.work_pending = true;
            }
            Err(mut failure) => {
                failure.publication.defer_retry();
                self.authored_schema_upgrade.view.retry_allowed = failure.publication.can_retry();
                self.authored_schema_upgrade.view.cancellation_allowed =
                    failure.publication.cancellation_allowed();
                self.authored_schema_upgrade.view.phase = P::RequiresReconcile;
                self.authored_schema_upgrade.view.work_pending = false;
                self.authored_schema_upgrade.view.refusal =
                    Some(if failure.publication.host_work_exhausted() {
                        EditorAuthoredSchemaUpgradeError::WorkLimit
                    } else {
                        failure.refusal
                    });
                self.authored_schema_upgrade.publication = Some(failure.publication);
            }
        }
    }

    fn install_authored_schema_upgrade(
        &mut self,
        mut publication: AuthoredSourceUpgradePublication,
    ) {
        // Final binding is deliberately local. The Host guards outlive their borrowed commits;
        // no self-referential aggregate or mutable whole-Session call crosses Ready.
        let result = bind_and_publish(
            &mut self.workspace,
            &mut self.persistence_authority,
            &mut self.scene_lanes,
            &mut self.prefab_lanes,
            &mut self.execution,
            &mut publication,
        );
        match result {
            Ok(()) => {
                self.authoring_prefab_source_observations = BTreeMap::new();
                self.prefab_content_refresh = BTreeMap::new();
                // All admitted replacement ownership was installed before this fence release.
                self.finish_supervised_play_retirement().expect(
                    "the acknowledged replacement owner stays registered throughout final adoption",
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
                self.authored_schema_upgrade
                    .finish(EditorAuthoredSchemaUpgradeOutcome::Adopted);
            }
            Err(()) => {
                self.authored_schema_upgrade.view.phase =
                    EditorAuthoredSchemaUpgradePhase::RequiresReconcile;
                self.authored_schema_upgrade.view.work_pending = false;
                self.observe_authored_schema_upgrade_publication_failure(&publication);
                self.authored_schema_upgrade.publication = Some(publication);
            }
        }
    }

    fn observe_authored_schema_upgrade_publication_failure(
        &mut self,
        publication: &AuthoredSourceUpgradePublication,
    ) {
        self.authored_schema_upgrade.view.retry_allowed = publication.can_retry();
        let (reason, code, summary) = if publication.host_work_exhausted() {
            (
                EditorAuthoredSchemaUpgradeError::WorkLimit,
                "project.editor.authored-upgrade-work-limit",
                "The complete upgrade exhausted its selected cumulative work allowance",
            )
        } else if publication.open_handles_exceeded() {
            (
                EditorAuthoredSchemaUpgradeError::OpenHandleLimit,
                "project.editor.authored-upgrade-open-handle-limit",
                "The complete source overlap exceeds the selected native-handle ceiling; select a larger finite ceiling for a new operation",
            )
        } else {
            (
                EditorAuthoredSchemaUpgradeError::Publication,
                "project.editor.authored-upgrade-publication-refused",
                "The reviewed publication requires reconciliation; original write admission remains fenced",
            )
        };
        self.authored_schema_upgrade.view.refusal = Some(reason);
        self.authored_schema_upgrade.diagnostics = Some(single_error(code, summary));
    }
}

enum HostGuard<'a> {
    Scene(persistence::RetainedHostGuard<'a, ScenePersistenceHost>),
    Prefab(persistence::RetainedHostGuard<'a, PrefabPersistenceHost>),
}

fn bind_and_publish(
    workspace: &mut EditorWorkspace,
    authority: &mut EditorPersistenceAuthority,
    scenes: &mut BTreeMap<EditorDocumentId, EditorScenePersistenceLane>,
    prefabs: &mut BTreeMap<EditorDocumentId, EditorPrefabPersistenceLane>,
    execution: &mut EditorExecutionLane,
    publication: &mut AuthoredSourceUpgradePublication,
) -> Result<(), ()> {
    let count = publication.sources().workspace().documents().len();
    // Collection, key and binding storage is admitted before allocating any final guards.
    let mut bytes = count
        .checked_mul(
            size_of::<HostGuard<'_>>()
                + size_of::<PersistenceAuthoredUpgradeCommit<'_>>()
                + size_of::<EditorAuthoredUpgradeSavedSource>()
                + size_of::<EditorDocumentId>(),
        )
        .ok_or(())?;
    for document in publication.sources().workspace().documents() {
        let prepared = publication.sources().workspace();
        let key = prepared
            .scene(document)
            .map(|source| source.source_key())
            .or_else(|| prepared.prefab(document).map(|source| source.source_key()))
            .ok_or(())?;
        let binding = prepared.source_binding(document).ok_or(())?;
        bytes = bytes
            .checked_add(key.retained_bytes())
            .and_then(|bytes| bytes.checked_add(binding.locator().len()))
            .ok_or(())?;
    }
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
    publication.release_installation_storage(bytes);
    result
}

fn bind_and_publish_admitted(
    workspace: &mut EditorWorkspace,
    authority: &mut EditorPersistenceAuthority,
    scenes: &mut BTreeMap<EditorDocumentId, EditorScenePersistenceLane>,
    prefabs: &mut BTreeMap<EditorDocumentId, EditorPrefabPersistenceLane>,
    execution: &mut EditorExecutionLane,
    publication: &mut AuthoredSourceUpgradePublication,
) -> Result<(), ()> {
    let count = publication.sources().workspace().documents().len();
    let mut observations = publication.take_observations();
    let mut documents = Vec::with_capacity(count);
    let mut saved = Vec::with_capacity(count);
    let mut guards = Vec::with_capacity(count);
    let mut missing_host = false;
    // Collections have the same document ordering as the complete Workspace preparation.
    for (document, observation) in publication
        .sources()
        .workspace()
        .documents()
        .zip(&observations)
    {
        let prepared = publication.sources().workspace();
        let (key, revision, state) = if let Some(source) = prepared.scene(document) {
            (
                source.source_key(),
                source.revision(),
                source.content_state(),
            )
        } else {
            let source = prepared.prefab(document).ok_or(())?;
            (
                source.source_key(),
                source.revision(),
                source.content_state(),
            )
        };
        assert_eq!(
            observation.document, document,
            "the complete successor set is ordered by document"
        );
        saved.push(EditorAuthoredUpgradeSavedSource::new(
            document,
            key.clone(),
            prepared.source_binding(document).ok_or(())?.clone(),
            revision,
            state,
            editor_digest(observation.source.digest()),
        ));
        documents.push(document);
        let guard = if let Some(lane) = scenes.get(&document) {
            lane.host.try_bind().map(HostGuard::Scene)
        } else {
            prefabs
                .get(&document)
                .and_then(|lane| lane.host.try_bind())
                .map(HostGuard::Prefab)
        };
        let Some(guard) = guard else {
            missing_host = true;
            break;
        };
        guards.push(guard);
    }
    if missing_host {
        publication.restore_observations(observations);
        return Err(());
    }
    let mut commits = Vec::with_capacity(count);
    let mut failed = None;
    for ((&document, guard), observation) in documents
        .iter()
        .zip(guards.iter_mut())
        .zip(observations.iter_mut())
    {
        let key = workspace
            .document_source_binding(document)
            .expect("the complete live set was revalidated");
        let lane_source = scenes
            .get(&document)
            .and_then(|lane| lane.authoring_source.as_ref())
            .or_else(|| {
                prefabs
                    .get(&document)
                    .and_then(|lane| lane.authoring_source.as_ref())
            })
            .expect("every upgrade source is bound");
        let predecessor = publication
            .sources()
            .predecessor_source(document, lane_source.relative_path())
            .expect("the complete preparation retains each captured predecessor");
        let expected = (
            predecessor.identity(),
            predecessor.parent_identity(),
            predecessor.digest(),
        );
        let cost = PersistenceAuthoredUpgradeValidationCost::checked(
            observation.source.digest(),
            predecessor.relative_path().len(),
        )
        .expect("admitted source lengths fit their original file limits");
        let reservation = match publication.admit_persistence_validation(cost) {
            Ok(bytes) => bytes,
            Err(error) => {
                failed = Some(error);
                break;
            }
        };
        let file = observation
            .file
            .take()
            .expect("one current replacement leaf per lane");
        let result = match guard {
            HostGuard::Scene(guard) => {
                guard
                    .host_mut()
                    .bind_authored_upgrade(expected, file, observation.source.digest())
            }
            HostGuard::Prefab(guard) => guard.host_mut().bind_authored_upgrade(
                key,
                expected,
                file,
                observation.source.digest(),
            ),
        };
        publication.release_installation_storage(reservation);
        match result {
            Ok(commit) => commits.push((document, commit)),
            Err(failure) => {
                observation.file = Some(failure.replacement);
                failed = Some(AuthoredSourceUpgradePublicationFailure::Persistence(
                    failure.reason,
                ));
                break;
            }
        }
    }
    if let Some(error) = failed {
        restore_files(commits, &mut observations);
        publication.restore_observations(observations);
        publication.record_failure(error);
        return Err(());
    }
    let (prepared, retained, bound_project) = publication.take_installation();
    let workspace_commit = match authority.bind_authored_schema_upgrade(
        workspace,
        prepared,
        saved,
        &mut publication.work,
    ) {
        Ok(commit) => commit,
        Err(failure) => {
            restore_files(commits, &mut observations);
            publication.restore_installation(failure.prepared, retained, bound_project);
            publication.restore_observations(observations);
            publication.record_failure(AuthoredSourceUpgradePublicationFailure::Workspace(
                failure.report,
            ));
            return Err(());
        }
    };
    match publication.publish_project(bound_project) {
        Ok(SourceUpgradeProjectOutcome::Published(published)) => {
            workspace_commit.commit();
            for (_, commit) in commits {
                commit.commit();
            }
            drop(guards);
            for observation in observations {
                if let Some(lane) = scenes.get_mut(&observation.document) {
                    lane.authoring_source = Some(observation.source);
                } else if let Some(lane) = prefabs.get_mut(&observation.document) {
                    lane.authoring_source = Some(observation.source);
                }
            }
            *execution = EditorExecutionLane::Authoring(published.session);
            drop(retained);
            Ok(())
        }
        refused => {
            let (prepared, _saved) = workspace_commit.into_prepared();
            restore_files(commits, &mut observations);
            let (bound, error) = match refused {
                Ok(SourceUpgradeProjectOutcome::Rejected { bound, reason }) => (
                    bound,
                    AuthoredSourceUpgradePublicationFailure::Rejected(reason),
                ),
                Ok(SourceUpgradeProjectOutcome::Failed { bound, stage }) => (
                    bound,
                    AuthoredSourceUpgradePublicationFailure::Failed(stage),
                ),
                Ok(SourceUpgradeProjectOutcome::Uncertain { bound, publication }) => (
                    bound,
                    AuthoredSourceUpgradePublicationFailure::Uncertain(publication),
                ),
                Err(failure) => (
                    failure.bound,
                    AuthoredSourceUpgradePublicationFailure::Project(failure.error),
                ),
                Ok(SourceUpgradeProjectOutcome::Published(_)) => unreachable!(),
            };
            publication.restore_installation(prepared, retained, bound);
            publication.restore_observations(observations);
            publication.record_failure(error);
            Err(())
        }
    }
}

fn restore_files(
    commits: Vec<(EditorDocumentId, PersistenceAuthoredUpgradeCommit<'_>)>,
    observations: &mut [crate::project_host::authoring::AuthoredSourceUpgradeObservedFile],
) {
    for (document, commit) in commits {
        let index = observations
            .binary_search_by_key(&document, |observation| observation.document)
            .expect("the original observation slot remains owned");
        observations[index].file = Some(commit.into_replacement());
    }
}
