use super::source_admission::SourceAdmissionError;
use super::*;

mod background;
mod reopen;
mod retirement;
#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
pub(super) use background::BackgroundSave;
#[cfg(test)]
pub(super) use background::SaveTestGate;
#[cfg(test)]
pub(crate) use background::{SaveTestControl, SaveTestPhase};
#[cfg(test)]
pub(super) use reopen::ReopenTestGate;
#[cfg(test)]
pub(crate) use reopen::ReopenTestPhase;
pub(super) use reopen::{BackgroundReopen, PendingReopen};
pub(super) use retirement::{HostCustody, HostCustodyPlugin, RetainedHost};

use nara_asset::{AssetPath, AssetRef};
use nara_scene::SceneEntityId;
use nara_tooling::EditorDocumentSourceBinding;

use crate::project_host::authoring::AuthoringDocumentAdmissionError;
use crate::project_host::persistence::{
    OpenedPrefabPersistence, OpenedPrefabReopen, PersistenceSaveOutcome, PrefabReopenOutcome,
    PrefabSaveCandidate,
};

#[derive(Default)]
pub(super) enum PersistenceState {
    #[default]
    Idle,
    Pending {
        attempt: EditorPersistenceAttemptId,
        operation: EditorPersistenceOperation,
        work: PendingPersistence,
    },
    Terminal {
        attempt: EditorPersistenceAttemptId,
        result: EditorPersistenceResult,
    },
}

impl PersistenceState {
    pub(super) const fn has_pending_work(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    pub(super) const fn view(&self) -> EditorPersistenceView {
        match *self {
            Self::Idle => EditorPersistenceView::Idle,
            Self::Pending {
                attempt, operation, ..
            } => EditorPersistenceView::Pending { attempt, operation },
            Self::Terminal { attempt, result } => {
                EditorPersistenceView::Terminal { attempt, result }
            }
        }
    }
}

#[derive(Clone)]
pub(super) enum PersistenceLane {
    Scene(EditorScenePersistenceLane),
    Prefab(EditorPrefabPersistenceLane),
    Extension(super::extension_documents::ExtensionPersistenceLane),
}

impl PersistenceLane {
    fn document(&self) -> EditorDocumentId {
        match self {
            Self::Scene(lane) => lane.document,
            Self::Prefab(lane) => lane.document,
            Self::Extension(lane) => lane.document,
        }
    }

    fn mark_uncertain(&self) {
        match self {
            Self::Scene(lane) => {
                lane.host.try_with(|host| host.mark_uncertain());
            }
            Self::Prefab(lane) => {
                lane.host.try_with(|host| host.mark_uncertain());
            }
            Self::Extension(lane) => {
                lane.host.try_with(|host| host.mark_uncertain());
            }
        }
    }
}

impl EditorProjectSession {
    #[cfg(test)]
    pub(crate) fn hold_next_save(&mut self, phase: SaveTestPhase) -> SaveTestControl {
        let (gate, control) = SaveTestGate::new(phase);
        self.next_save_gate = Some(gate);
        control
    }

    pub(super) fn advance_persistence(&mut self) {
        let state = std::mem::take(&mut self.persistence);
        let PersistenceState::Pending {
            attempt,
            operation,
            work,
        } = state
        else {
            self.persistence = state;
            return;
        };
        let result = match work {
            PendingPersistence::Save {
                lane,
                candidate,
                authoring,
            } => {
                self.persistence = PersistenceState::Pending {
                    attempt,
                    operation,
                    work: PendingPersistence::Saving(BackgroundSave::start(
                        lane,
                        candidate,
                        authoring,
                        self.authoring_runtime.persistence_tasks(),
                        #[cfg(test)]
                        self.next_save_gate.take(),
                    )),
                };
                return;
            }
            PendingPersistence::Saving(mut worker) => {
                let Some((lane, outcome)) = worker.poll() else {
                    self.persistence = PersistenceState::Pending {
                        attempt,
                        operation,
                        work: PendingPersistence::Saving(worker),
                    };
                    return;
                };
                let refresh = match &lane {
                    PersistenceLane::Prefab(lane) => lane
                        .host
                        .try_with(|host| host.publishes_runtime_content())
                        .unwrap_or(false),
                    PersistenceLane::Scene(_) | PersistenceLane::Extension(_) => false,
                };
                self.acknowledge_save(lane, refresh, outcome)
            }
            PendingPersistence::Reopen(pending) => {
                self.persistence = PersistenceState::Pending {
                    attempt,
                    operation,
                    work: PendingPersistence::Reopening(BackgroundReopen::start(
                        pending,
                        self.authoring_runtime.persistence_tasks(),
                        #[cfg(test)]
                        self.next_reopen_gate.take(),
                    )),
                };
                return;
            }
            PendingPersistence::Reopening(mut worker) => {
                let Some(completion) = worker.poll() else {
                    self.persistence = PersistenceState::Pending {
                        attempt,
                        operation,
                        work: PendingPersistence::Reopening(worker),
                    };
                    return;
                };
                self.publish_reopen_completion(worker.admission(), completion)
            }
        };
        self.persistence = PersistenceState::Terminal { attempt, result };
    }

    fn capture_persistence_lane(
        &self,
        document: EditorDocumentId,
    ) -> Result<PersistenceLane, EditorPersistenceRejection> {
        let lane = match self.workspace.document_kind(document) {
            Some(EditorDocumentKind::Scene) => self
                .scene_lanes
                .get(&document)
                .cloned()
                .map(PersistenceLane::Scene),
            Some(EditorDocumentKind::Prefab) => self
                .prefab_lanes
                .get(&document)
                .cloned()
                .map(PersistenceLane::Prefab),
            Some(EditorDocumentKind::Extension(_)) => self
                .extension_documents
                .lane(document)
                .map(PersistenceLane::Extension),
            None => return Err(EditorPersistenceRejection::MissingDocument),
        };
        lane.ok_or(EditorPersistenceRejection::SourceBindingMismatch)
    }

    pub(super) fn prefab_source_key(&self, document: EditorDocumentId) -> Option<AssetPath> {
        let lane = self.prefab_lanes.get(&document)?;
        let directory = match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => snapshot.settings().paths.prefabs.as_str(),
            EditorExecutionLane::TrustedCompiled(lane) => {
                lane.content_candidate.settings().paths.prefabs.as_str()
            }
        };
        let source = lane
            .project_path
            .strip_prefix(directory)?
            .strip_prefix('/')?;
        AssetPath::new(source.to_owned()).ok()
    }
    #[doc(hidden)]
    pub(super) fn open_prefab_source(
        &mut self,
        document: EditorDocumentId,
        base_revision: nara_scene::SceneAuthoringRevision,
        entity: &SceneEntityId,
    ) -> EditorWorkspaceCommandReport {
        if !self.accepts_new_work()
            || self.schema_adoption_fenced()
            || self.persistence.has_pending_work()
            || self.pending_workspace_intent.is_some()
            || self.pending_document_open.is_some()
        {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.document-open-busy",
            );
        }
        let source_binding = match self.prefab_source_binding(document, base_revision, entity) {
            Ok(source_binding) => source_binding,
            Err(code) => {
                return editor_workspace_command_rejected(self.workspace.active_document(), code);
            }
        };
        let prefab_directory = match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => snapshot.settings().paths.prefabs.as_str(),
            EditorExecutionLane::TrustedCompiled(lane) => {
                lane.content_candidate.settings().paths.prefabs.as_str()
            }
        };
        let project_path: Arc<str> =
            Arc::from(format!("{prefab_directory}/{}", source_binding.locator()));
        if let Some(document) = self.exact_source_owner(&project_path) {
            return if self.workspace.document_kind(document) == Some(EditorDocumentKind::Prefab) {
                self.workspace.activate_document(document)
            } else {
                editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.document-source-already-open",
                )
            };
        }
        let exclusion = match &self.execution {
            EditorExecutionLane::Authoring(_) => self.exclude_project_source_aliases(&project_path),
            EditorExecutionLane::TrustedCompiled(lane) => {
                RelativePath::new(source_binding.locator())
                    .map_err(|_| SourceAdmissionError::SourceRead)
                    .and_then(|path| {
                        lane.prefab_root
                            .open_file(&path)
                            .map_err(|_| SourceAdmissionError::SourceRead)
                    })
                    .and_then(|file| self.exclude_source_aliases(&project_path, &file))
            }
        };
        if let Err(error) = exclusion {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                error.code(),
            );
        }
        if self.authoring_project().is_some() {
            return self.open_authoring_prefab_source(source_binding);
        }
        let Some(lane) = self.trusted_compiled() else {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-runtime-lane-unavailable",
            );
        };
        let Some(expected_source_key) = lane
            .snapshot
            .prefabs()
            .iter()
            .find(|prefab| prefab.path().as_str() == source_binding.locator())
            .map(|prefab| prefab.source_key().clone())
        else {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-source-outside-content-closure",
            );
        };
        if let Err(code) = self.exclude_native_source_key(&project_path, &expected_source_key) {
            return editor_workspace_command_rejected(self.workspace.active_document(), code);
        }
        let opened = PrefabPersistenceHost::open(
            &lane.prefab_root,
            source_binding.clone(),
            lane.plan.schema_validation().registry(),
        );
        let OpenedPrefabPersistence {
            host,
            session,
            digest,
        } = match opened {
            Ok(opened) => opened,
            Err(stage) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    prefab_open_failure_code(stage),
                );
            }
        };
        let source_key = match host
            .read_source()
            .open()
            .map_err(|_| SourceAdmissionError::SourceRead)
        {
            Ok(file) => {
                let policy =
                    crate::project_host::authoring::native_source::NativeDocumentSourcePolicy::new(
                        lane.content_candidate.settings(),
                    );
                match policy.admit_in_directory(
                    &lane.prefab_root,
                    &lane.content_candidate.settings().paths.prefabs,
                    source_binding.locator(),
                    nara_scene::SourceDocumentKind::Prefab,
                    file.identity(),
                ) {
                    Ok(admitted) if admitted.key() == &expected_source_key => {
                        admitted.key().clone()
                    }
                    Ok(_) => {
                        return editor_workspace_command_rejected(
                            self.workspace.active_document(),
                            "project.editor.prefab-source-identity-changed",
                        );
                    }
                    Err(error) => {
                        return editor_workspace_command_rejected(
                            self.workspace.active_document(),
                            error.code(),
                        );
                    }
                }
            }
            Err(error) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    error.code(),
                );
            }
        };
        if let Err(error) = host
            .read_source()
            .open()
            .map_err(|_| SourceAdmissionError::SourceRead)
            .and_then(|file| self.exclude_source_aliases(&project_path, &file))
        {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                error.code(),
            );
        }
        let title = source_binding.locator().to_owned();
        let previous_active = self.workspace.active_document();
        let report = match self
            .workspace
            .open_prefab_session(title, source_binding, session)
        {
            Ok(report) => report,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-publication-failed",
                );
            }
        };
        let Some(document) = report.opened_document else {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-publication-failed",
            );
        };
        let binding = self.bind_opened_prefab_source_digest(document, digest);
        if !binding.applied {
            self.discard_unpublished_document(document, previous_active);
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-persistence-binding-failed",
            );
        }
        let Some(host) = self.authoring_runtime.host_custody().retain_prefab(host) else {
            self.discard_unpublished_document(document, previous_active);
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.document-open-busy",
            );
        };
        self.prefab_lanes.insert(
            document,
            EditorPrefabPersistenceLane {
                project_path,
                source_key,
                authoring_source: None,
                document,
                host,
            },
        );
        report
    }

    fn open_authoring_prefab_source(
        &mut self,
        source_binding: EditorDocumentSourceBinding,
    ) -> EditorWorkspaceCommandReport {
        let path = match AssetPath::new(source_binding.locator().to_owned()) {
            Ok(path) => path,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-source-path-invalid",
                );
            }
        };
        let (project_root, prefab_directory, snapshot_observation, native_source, published) = {
            let Some(snapshot) = self.authoring_project() else {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-authoring-lane-unavailable",
                );
            };
            let Some(prefab) = snapshot.prefab(&path) else {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-source-outside-authoring-closure",
                );
            };
            let published = match snapshot.fork_prefab(&path) {
                Ok(Some(published)) => published,
                Ok(None) => {
                    return editor_workspace_command_rejected(
                        self.workspace.active_document(),
                        "project.editor.prefab-source-outside-authoring-closure",
                    );
                }
                Err(error) => {
                    let error = AuthoringDocumentAdmissionError::Document(error);
                    let _ = self.diagnostics.push(error.diagnostic());
                    return editor_workspace_command_rejected(
                        self.workspace.active_document(),
                        error.code(),
                    );
                }
            };
            (
                snapshot.project_root(),
                snapshot.settings().paths.prefabs.clone(),
                AuthoringPrefabSourceObservation::new(
                    prefab.source().identity(),
                    prefab.source().digest(),
                ),
                prefab.native_source().clone(),
                published,
            )
        };
        let observation = self
            .authoring_prefab_source_observations
            .get(&path)
            .copied()
            .unwrap_or(snapshot_observation);
        let directory_path = match RelativePath::new(prefab_directory.as_str()) {
            Ok(path) => path,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefabs-path-invalid",
                );
            }
        };
        let prefabs = match project_root.open_directory(&directory_path) {
            Ok(prefabs) => prefabs,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefabs-open-failed",
                );
            }
        };
        let source_path = match RelativePath::new(source_binding.locator()) {
            Ok(path) => path,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-source-path-invalid",
                );
            }
        };
        // The captured observation is historical. Acquire a live leaf and keep it in the
        // candidate Host while the fresh bytes pass authoring readmission below.
        let source_file = match prefabs.open_file_exact(&source_path) {
            Ok(file) if file.identity() == observation.identity => file,
            Ok(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    prefab_open_failure_code(EditorPersistenceFailureStage::Validate),
                );
            }
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    prefab_open_failure_code(EditorPersistenceFailureStage::OpenTarget),
                );
            }
        };
        let (mut host, _) = match PrefabPersistenceHost::bind_authoring_source(
            &prefabs,
            source_binding.clone(),
            source_file,
            observation.digest,
        ) {
            Ok(bound) => bound,
            Err(stage) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    prefab_open_failure_code(stage),
                );
            }
        };
        let diagnostics = published.diagnostics().clone();
        let basis =
            nara_scene::PrefabAuthoringSession::from_published_authoring_document(published);
        let prepared = match host.prepare_reopen(basis.authoring_readmission()) {
            Ok(request) => request.prepare(),
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-preservation-mismatch",
                );
            }
        };
        let reopened = match prepared.admit(None) {
            PrefabReopenOutcome::Opened(opened) => opened,
            PrefabReopenOutcome::Rejected(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-preservation-mismatch",
                );
            }
            PrefabReopenOutcome::Failed(stage) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    prefab_open_failure_code(stage),
                );
            }
        };
        let OpenedPrefabReopen {
            session,
            digest: opened_digest,
            file,
            content_digest,
        } = *reopened;
        let identity = file.identity();
        host.commit_reopen(file, content_digest);
        let project_path: Arc<str> = Arc::from(format!(
            "{}/{}",
            prefab_directory.as_str(),
            source_binding.locator()
        ));
        if let Err(error) = native_source.revalidate(&project_root) {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                error.code(),
            );
        }
        let source_key = native_source.key().clone();
        let authoring_source = {
            let Some(project) = self.authoring_project() else {
                unreachable!("authoring Prefab Open retains its project");
            };
            let captured = project
                .prefab(&path)
                .expect("the admitted Prefab source is retained");
            match crate::project_host::authoring::document::AuthoringDocumentSourceFact::from_captured_source(
                captured.source(), captured.native_source(),
            ).and_then(|fact| fact.with_persistence_observation(
                identity,
                host.accepted_source_observation().and_then(|(_, parent, _)| parent),
                content_digest,
            )) {
                Ok(source) => source,
                Err(error) => return editor_workspace_command_rejected(
                    self.workspace.active_document(), error.code(),
                ),
            }
        };
        if let Err(code) = self.exclude_native_source_key(&project_path, &source_key) {
            return editor_workspace_command_rejected(self.workspace.active_document(), code);
        }
        if let Err(error) = host
            .read_source()
            .open()
            .map_err(|_| SourceAdmissionError::SourceRead)
            .and_then(|file| self.exclude_source_aliases(&project_path, &file))
        {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                error.code(),
            );
        }
        let title = source_binding.locator().to_owned();
        let previous_active = self.workspace.active_document();
        let report = match self
            .workspace
            .open_prefab_session(title, source_binding, session)
        {
            Ok(report) => report,
            Err(_) => {
                return editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.prefab-publication-failed",
                );
            }
        };
        let Some(document) = report.opened_document else {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-publication-failed",
            );
        };
        let binding = self.bind_opened_prefab_source_digest(document, opened_digest);
        if !binding.applied {
            self.discard_unpublished_document(document, previous_active);
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.prefab-persistence-binding-failed",
            );
        }
        let _ = self.diagnostics.extend(diagnostics);
        self.authoring_prefab_source_observations.insert(
            path,
            AuthoringPrefabSourceObservation::new(identity, content_digest),
        );
        let Some(host) = self.authoring_runtime.host_custody().retain_prefab(host) else {
            self.discard_unpublished_document(document, previous_active);
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.document-open-busy",
            );
        };
        self.prefab_lanes.insert(
            document,
            EditorPrefabPersistenceLane {
                project_path,
                source_key,
                authoring_source: Some(authoring_source),
                document,
                host,
            },
        );
        report
    }

    fn prefab_source_binding(
        &self,
        document: EditorDocumentId,
        base_revision: nara_scene::SceneAuthoringRevision,
        entity: &SceneEntityId,
    ) -> Result<EditorDocumentSourceBinding, &'static str> {
        let scene = self
            .workspace
            .scene(document)
            .ok_or("project.editor.prefab-source-requires-scene")?;
        if scene.revision() != base_revision {
            return Err("project.editor.prefab-source-stale-scene");
        }
        if scene.selection().top_entity() != Some(entity) {
            return Err("project.editor.prefab-source-not-selected");
        }
        let record = scene
            .session()
            .document()
            .entities
            .iter()
            .find(|record| &record.id == entity)
            .ok_or("project.editor.prefab-source-entity-missing")?;
        let instance = record
            .prefab
            .as_ref()
            .ok_or("project.editor.prefab-source-not-an-anchor")?;
        let resolved_source;
        let source = match &instance.source {
            AssetRef::Path(source) => source,
            reference @ AssetRef::StableId(_) => {
                resolved_source = match &self.execution {
                    EditorExecutionLane::Authoring(snapshot) => snapshot
                        .asset_database()
                        .resolve_ref(reference)
                        .ok()
                        .map(|record| record.path().clone()),
                    EditorExecutionLane::TrustedCompiled(lane) => {
                        lane.snapshot.resolved_asset_path(reference).cloned()
                    }
                }
                .ok_or("project.editor.prefab-source-stable-id-unresolved")?;
                &resolved_source
            }
        };
        let source_is_admitted = match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => snapshot.prefab(source).is_some(),
            EditorExecutionLane::TrustedCompiled(lane) => lane
                .snapshot
                .prefabs()
                .iter()
                .any(|prefab| prefab.path() == source),
        };
        if !source_is_admitted {
            return Err("project.editor.prefab-source-outside-content-closure");
        }
        Ok(EditorDocumentSourceBinding::new(source.as_str().to_owned()))
    }

    #[cfg(test)]
    pub(crate) fn test_reject_next_valid_persistence_receipt(
        &mut self,
        document: EditorDocumentId,
    ) {
        self.scene_lanes
            .get(&document)
            .expect("the persistence test requires an open Scene lane")
            .host
            .try_with(|host| host.test_reject_next_valid_receipt())
            .unwrap();
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_save(
        &mut self,
        document: EditorDocumentId,
        stage: EditorPersistenceFailureStage,
    ) {
        self.scene_lanes
            .get(&document)
            .expect("the persistence test requires an open Scene lane")
            .host
            .try_with(|host| host.test_fail_next_save(stage))
            .unwrap();
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_save_after_write(
        &mut self,
        document: EditorDocumentId,
    ) -> crate::project_host::persistence::CleanupTestControl {
        self.scene_lanes
            .get(&document)
            .expect("the test requires an open Scene lane")
            .host
            .try_with(|host| host.test_fail_after_write())
            .unwrap()
    }

    #[cfg(test)]
    pub(crate) fn test_allow_save_cleanup(&mut self, document: EditorDocumentId) {
        self.scene_lanes
            .get(&document)
            .expect("the test requires an open Scene lane")
            .host
            .try_with(|host| host.test_allow_cleanup())
            .unwrap();
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_prefab_save_after_write(
        &mut self,
        document: EditorDocumentId,
    ) -> crate::project_host::persistence::CleanupTestControl {
        self.prefab_lanes
            .get(&document)
            .expect("the test requires an open Prefab lane")
            .host
            .try_with(|host| host.test_fail_after_write())
            .unwrap()
    }

    #[cfg(test)]
    pub(crate) fn test_reject_next_prefab_source_binding(&mut self) {
        self.reject_next_prefab_source_binding = true;
    }

    fn bind_opened_prefab_source_digest(
        &mut self,
        document: EditorDocumentId,
        digest: nara_tooling::EditorDocumentDigest,
    ) -> EditorWorkspaceCommandReport {
        #[cfg(test)]
        if std::mem::take(&mut self.reject_next_prefab_source_binding) {
            return EditorWorkspaceCommandReport::default();
        }
        self.persistence_authority
            .bind_opened_source_digest(&mut self.workspace, document, digest)
    }

    #[must_use]
    #[doc(hidden)]
    /// Immediate read-only Host state, including an admitted operation before frame publication.
    pub const fn persistence_view(&self) -> EditorPersistenceView {
        self.persistence.view()
    }

    pub(crate) fn request_persistence(
        &mut self,
        command: EditorPersistenceCommand,
    ) -> EditorPersistenceRequestResult {
        if !matches!(command, EditorPersistenceCommand::AcknowledgeResult { .. })
            && (self.pending_workspace_intent.is_some() || self.pending_document_open.is_some())
        {
            return EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy);
        }
        self.request_persistence_after_workspace_arbitration(command)
    }

    pub(super) fn request_persistence_for_workspace_intent(
        &mut self,
        command: EditorPersistenceCommand,
    ) -> EditorPersistenceRequestResult {
        self.request_persistence_after_workspace_arbitration(command)
    }

    fn request_persistence_after_workspace_arbitration(
        &mut self,
        command: EditorPersistenceCommand,
    ) -> EditorPersistenceRequestResult {
        if let EditorPersistenceCommand::AcknowledgeResult { attempt } = command {
            return self.acknowledge_persistence_attempt(attempt);
        }
        if !self.source_tools.reclamation_settled() {
            return EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy);
        }
        if !self.accepts_new_work()
            || self.schema_adoption_fenced()
            || self.persistence.has_pending_work()
        {
            return EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy);
        }
        let (request, requested_document) = match command {
            EditorPersistenceCommand::Save { document } => (
                EditorPersistenceRequestKind::Save,
                document.or(self.workspace.active_document()),
            ),
            EditorPersistenceCommand::Reopen { document } => (
                EditorPersistenceRequestKind::Reopen,
                document.or(self.workspace.active_document()),
            ),
            EditorPersistenceCommand::AcknowledgeResult { .. } => unreachable!(),
        };
        if self.persistence_view().result().is_some_and(|result| {
            !persistence_result_allows_retry(result, request, requested_document)
        }) {
            return EditorPersistenceRequestResult::Refused(
                EditorPersistenceRejection::ResultPending,
            );
        }

        let Some(sequence) = self.next_persistence_attempt.checked_add(1) else {
            return EditorPersistenceRequestResult::Refused(
                EditorPersistenceRejection::AttemptIdExhausted,
            );
        };
        let attempt = EditorPersistenceAttemptId::new(self.current_frame().revision(), sequence);
        self.next_persistence_attempt = sequence;
        let Some(requested_document) = requested_document else {
            return self.reject_persistence_attempt(
                attempt,
                request,
                None,
                EditorPersistenceRejection::NoActiveDocument,
            );
        };
        let runtime_blocks_reopen = self.scene_runtime_blocks_reopen();
        if matches!(command, EditorPersistenceCommand::Reopen { .. })
            && self.workspace.document_kind(requested_document) == Some(EditorDocumentKind::Scene)
            && runtime_blocks_reopen
        {
            return self.reject_persistence_attempt(
                attempt,
                request,
                Some(requested_document),
                EditorPersistenceRejection::RuntimeActive,
            );
        }

        match command {
            EditorPersistenceCommand::Save { .. } => {
                let Some(kind) = self.workspace.document_kind(requested_document) else {
                    return self.reject_persistence_attempt(
                        attempt,
                        request,
                        Some(requested_document),
                        EditorPersistenceRejection::MissingDocument,
                    );
                };
                if !self
                    .workspace
                    .document_is_dirty(requested_document)
                    .unwrap_or(false)
                {
                    return self.reject_persistence_attempt(
                        attempt,
                        request,
                        Some(requested_document),
                        EditorPersistenceRejection::NotDirty,
                    );
                }
                let lane = match self.capture_persistence_lane(requested_document) {
                    Ok(lane) => lane,
                    Err(reason) => {
                        return self.reject_persistence_attempt(
                            attempt,
                            request,
                            Some(requested_document),
                            reason,
                        );
                    }
                };
                let Some(checkpoint) = self
                    .persistence_authority
                    .capture(&self.workspace, requested_document)
                else {
                    return self.reject_persistence_attempt(
                        attempt,
                        request,
                        Some(requested_document),
                        EditorPersistenceRejection::StaleRevision,
                    );
                };
                let revision = checkpoint.revision();
                if let Some(Err(error)) = checkpoint.extension_bytes() {
                    self.diagnostics.push(nara_diagnostic::Diagnostic::error(
                        error.code(),
                        error.summary(),
                    ));
                }
                let authoring = match kind {
                    EditorDocumentKind::Scene => self
                        .workspace
                        .scene(requested_document)
                        .and_then(|slot| slot.session().authoring_readmission()),
                    EditorDocumentKind::Prefab => self
                        .workspace
                        .prefab(requested_document)
                        .and_then(|slot| slot.session().authoring_readmission()),
                    EditorDocumentKind::Extension(_) => None,
                };
                let work = PendingPersistence::Save {
                    lane,
                    candidate: match kind {
                        EditorDocumentKind::Scene => {
                            PendingSave::Scene(SceneSaveCandidate { checkpoint })
                        }
                        EditorDocumentKind::Prefab => {
                            PendingSave::Prefab(PrefabSaveCandidate { checkpoint })
                        }
                        EditorDocumentKind::Extension(_) => PendingSave::Extension(checkpoint),
                    },
                    authoring,
                };
                let operation = EditorPersistenceOperation::Saving {
                    document: requested_document,
                    captured_revision: revision,
                };
                self.persistence = PersistenceState::Pending {
                    attempt,
                    operation,
                    work,
                };
            }
            EditorPersistenceCommand::Reopen { .. } => {
                let pending = match self.capture_reopen(requested_document) {
                    Ok(pending) => pending,
                    Err(reason) => {
                        return self.reject_persistence_attempt(
                            attempt,
                            request,
                            Some(requested_document),
                            reason,
                        );
                    }
                };
                let operation = EditorPersistenceOperation::Opening {
                    document: requested_document,
                };
                self.persistence = PersistenceState::Pending {
                    attempt,
                    operation,
                    work: PendingPersistence::Reopen(pending),
                };
            }
            EditorPersistenceCommand::AcknowledgeResult { .. } => unreachable!(),
        }
        EditorPersistenceRequestResult::Accepted { attempt }
    }

    fn reject_persistence_attempt(
        &mut self,
        attempt: EditorPersistenceAttemptId,
        request: EditorPersistenceRequestKind,
        document: Option<EditorDocumentId>,
        reason: EditorPersistenceRejection,
    ) -> EditorPersistenceRequestResult {
        self.persistence = PersistenceState::Terminal {
            attempt,
            result: EditorPersistenceResult::Rejected {
                request,
                document,
                reason,
            },
        };
        EditorPersistenceRequestResult::Rejected { attempt, reason }
    }

    fn acknowledge_persistence_attempt(
        &mut self,
        expected: EditorPersistenceAttemptId,
    ) -> EditorPersistenceRequestResult {
        match self.persistence_view() {
            EditorPersistenceView::Terminal { attempt, .. } if attempt == expected => {
                self.persistence = PersistenceState::Idle;
                EditorPersistenceRequestResult::Acknowledged { attempt }
            }
            EditorPersistenceView::Pending { .. } => {
                EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
            }
            _ => {
                EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::ResultMismatch)
            }
        }
    }

    pub(super) fn advance_workspace_persistence(&mut self) {
        let Some(mut pending) = self.pending_workspace_intent.take() else {
            return;
        };
        let Some(expected_attempt) = pending.save_attempt() else {
            self.pending_workspace_intent = Some(pending);
            return;
        };
        let Some(pending_document) = pending.decision_document() else {
            self.pending_workspace_intent = Some(pending);
            return;
        };
        let result = match self.persistence_view() {
            EditorPersistenceView::Terminal { attempt, result } if attempt == expected_attempt => {
                Some(result)
            }
            _ => None,
        };
        match result {
            Some(EditorPersistenceResult::Saved { document, .. })
                if document == pending_document =>
            {
                match &mut pending {
                    PendingWorkspaceIntent::CloseDocument(close) => {
                        close.phase = WorkspaceIntentProgress::RetiringRuntime;
                    }
                    PendingWorkspaceIntent::Exit(exit) => {
                        let _ = self.acknowledge_persistence_attempt(expected_attempt);
                        exit.cursor = exit.cursor.saturating_add(1);
                        self.prepare_current_exit_document(exit);
                    }
                }
            }
            Some(
                EditorPersistenceResult::Rejected { .. }
                | EditorPersistenceResult::Failed { .. }
                | EditorPersistenceResult::PersistenceUncertain { .. },
            ) => {
                pending.set_progress(WorkspaceIntentProgress::AwaitingDecision);
            }
            Some(EditorPersistenceResult::Opened { .. })
            | Some(EditorPersistenceResult::Saved { .. }) => {
                pending.set_progress(WorkspaceIntentProgress::AwaitingDecision);
            }
            None => {}
        }
        if pending.phase() == EditorWorkspaceIntentPhase::AwaitingDecision {
            self.refresh_workspace_close_prompt(&mut pending);
        }
        self.pending_workspace_intent = Some(pending);
    }

    fn acknowledge_save(
        &mut self,
        lane: PersistenceLane,
        refresh_runtime_content: bool,
        outcome: PersistenceSaveOutcome,
    ) -> EditorPersistenceResult {
        let document = lane.document();
        match outcome {
            PersistenceSaveOutcome::Saved { commit, evidence } => {
                let report = self
                    .persistence_authority
                    .commit(&mut self.workspace, commit);
                if !report.applied {
                    lane.mark_uncertain();
                    return EditorPersistenceResult::PersistenceUncertain {
                        document: evidence.document(),
                        revision: evidence.revision(),
                        digest: evidence.digest(),
                    };
                }
                if refresh_runtime_content {
                    self.schedule_prefab_content_refresh(evidence);
                } else if matches!(lane, PersistenceLane::Prefab(_)) {
                    self.commit_authoring_prefab_source_observation(document, evidence);
                }
                self.refresh_current_saved_source_digest();
                EditorPersistenceResult::Saved {
                    document: evidence.document(),
                    revision: evidence.revision(),
                    digest: evidence.digest(),
                }
            }
            PersistenceSaveOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                reason,
            },
            PersistenceSaveOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Save,
                document: Some(document),
                stage,
            },
            PersistenceSaveOutcome::PersistenceUncertain { evidence } => {
                EditorPersistenceResult::PersistenceUncertain {
                    document: evidence.document(),
                    revision: evidence.revision(),
                    digest: evidence.digest(),
                }
            }
        }
    }

    fn publish_scene_reopen(
        &mut self,
        lane: &EditorScenePersistenceLane,
        base_revision: nara_scene::SceneAuthoringRevision,
        native_source: &crate::project_host::authoring::native_source::AdmittedNativeDocumentSource,
        reopened: SceneReopenOutcome,
    ) -> EditorPersistenceResult {
        let document = lane.document;
        match reopened {
            SceneReopenOutcome::Opened(opened) => {
                let OpenedSceneReopen {
                    session,
                    digest,
                    file,
                    content_digest,
                } = *opened;
                let identity = file.identity();
                if let Err(reason) = self.validate_reopened_native_source(
                    &lane.project_path,
                    &lane.source_key,
                    identity,
                    native_source,
                ) {
                    return EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Reopen,
                        document: Some(document),
                        reason,
                    };
                }
                let refreshed_source = match lane
                    .authoring_source
                    .as_ref()
                    .map(|source| {
                        source
                            .with_persistence_observation(
                                identity,
                                source.parent_identity(),
                                content_digest,
                            )
                            .and_then(|source| source.with_native_source(native_source.clone()))
                    })
                    .transpose()
                {
                    Ok(source) => source,
                    Err(_) => {
                        return EditorPersistenceResult::Rejected {
                            request: EditorPersistenceRequestKind::Reopen,
                            document: Some(document),
                            reason: EditorPersistenceRejection::SourceBindingMismatch,
                        };
                    }
                };
                let report = lane.host.try_with(|host| {
                    let report = self.persistence_authority.publish_reopened_session(
                        &mut self.workspace,
                        document,
                        base_revision,
                        session,
                        digest,
                    );
                    if report.is_ok() {
                        host.commit_reopen(file, content_digest);
                    }
                    report
                });
                let report = match report {
                    Some(Ok(report)) => report,
                    Some(Err(_)) => {
                        return EditorPersistenceResult::Rejected {
                            request: EditorPersistenceRequestKind::Reopen,
                            document: Some(document),
                            reason: EditorPersistenceRejection::StaleRevision,
                        };
                    }
                    None => {
                        return EditorPersistenceResult::Rejected {
                            request: EditorPersistenceRequestKind::Reopen,
                            document: Some(document),
                            reason: EditorPersistenceRejection::Busy,
                        };
                    }
                };
                let revision = report
                    .revision
                    .expect("a reopened document reports its authoring revision");
                if let Some(stored) = self.scene_lanes.get_mut(&document) {
                    stored.authoring_source = refreshed_source;
                }
                EditorPersistenceResult::Opened {
                    document,
                    revision,
                    digest,
                }
            }
            SceneReopenOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason,
            },
            SceneReopenOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                stage,
            },
        }
    }

    fn publish_prefab_reopen(
        &mut self,
        lane: &EditorPrefabPersistenceLane,
        base_revision: nara_scene::SceneAuthoringRevision,
        source_binding: EditorDocumentSourceBinding,
        refresh_runtime_content: bool,
        native_source: &crate::project_host::authoring::native_source::AdmittedNativeDocumentSource,
        reopened: PrefabReopenOutcome,
    ) -> EditorPersistenceResult {
        let document = lane.document;
        match reopened {
            PrefabReopenOutcome::Opened(opened) => {
                let OpenedPrefabReopen {
                    session,
                    digest,
                    file,
                    content_digest,
                } = *opened;
                let identity = file.identity();
                if let Err(reason) = self.validate_reopened_native_source(
                    &lane.project_path,
                    &lane.source_key,
                    identity,
                    native_source,
                ) {
                    return EditorPersistenceResult::Rejected {
                        request: EditorPersistenceRequestKind::Reopen,
                        document: Some(document),
                        reason,
                    };
                }
                let refreshed_source = match lane
                    .authoring_source
                    .as_ref()
                    .map(|source| {
                        source
                            .with_persistence_observation(
                                identity,
                                source.parent_identity(),
                                content_digest,
                            )
                            .and_then(|source| source.with_native_source(native_source.clone()))
                    })
                    .transpose()
                {
                    Ok(source) => source,
                    Err(_) => {
                        return EditorPersistenceResult::Rejected {
                            request: EditorPersistenceRequestKind::Reopen,
                            document: Some(document),
                            reason: EditorPersistenceRejection::SourceBindingMismatch,
                        };
                    }
                };
                let report = lane.host.try_with(|host| {
                    let report = self.persistence_authority.publish_reopened_prefab_session(
                        &mut self.workspace,
                        document,
                        base_revision,
                        source_binding,
                        session,
                        digest,
                    );
                    if report.is_ok() {
                        host.commit_reopen(file, content_digest);
                    }
                    report
                });
                let report = match report {
                    Some(Ok(report)) => report,
                    Some(Err(_)) => {
                        return EditorPersistenceResult::Rejected {
                            request: EditorPersistenceRequestKind::Reopen,
                            document: Some(document),
                            reason: EditorPersistenceRejection::StaleRevision,
                        };
                    }
                    None => {
                        return EditorPersistenceResult::Rejected {
                            request: EditorPersistenceRequestKind::Reopen,
                            document: Some(document),
                            reason: EditorPersistenceRejection::Busy,
                        };
                    }
                };
                let revision = report
                    .revision
                    .expect("a reopened Prefab reports its authoring revision");
                if let Some(stored) = self.prefab_lanes.get_mut(&document) {
                    stored.authoring_source = refreshed_source;
                }
                if refresh_runtime_content {
                    self.schedule_reopened_prefab_content_refresh(
                        document,
                        revision,
                        content_digest,
                    );
                } else if let Some(path) = self.prefab_source_key(document) {
                    self.authoring_prefab_source_observations.insert(
                        path,
                        AuthoringPrefabSourceObservation::new(identity, content_digest),
                    );
                }
                EditorPersistenceResult::Opened {
                    document,
                    revision,
                    digest,
                }
            }
            PrefabReopenOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                reason,
            },
            PrefabReopenOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                request: EditorPersistenceRequestKind::Reopen,
                document: Some(document),
                stage,
            },
        }
    }

    fn commit_authoring_prefab_source_observation(
        &mut self,
        document: EditorDocumentId,
        evidence: EditorPersistenceReceipt,
    ) {
        let Some(path) = self.prefab_source_key(document) else {
            return;
        };
        let (Some(identity), Some(digest)) =
            (evidence.published_identity(), evidence.published_digest())
        else {
            return;
        };
        self.authoring_prefab_source_observations.insert(
            path,
            AuthoringPrefabSourceObservation::new(
                identity,
                ContentDigest::from_parts(digest.encoded_bytes(), digest.hash()),
            ),
        );
    }
}

fn prefab_open_failure_code(stage: EditorPersistenceFailureStage) -> &'static str {
    match stage {
        EditorPersistenceFailureStage::OpenTarget => "project.editor.prefab-open-failed",
        EditorPersistenceFailureStage::ReadTarget => "project.editor.prefab-read-failed",
        EditorPersistenceFailureStage::Decode => "project.editor.prefab-decode-failed",
        EditorPersistenceFailureStage::Validate => "project.editor.prefab-validation-failed",
        EditorPersistenceFailureStage::Encode
        | EditorPersistenceFailureStage::CreateTemporary
        | EditorPersistenceFailureStage::WriteTemporary
        | EditorPersistenceFailureStage::SyncTemporary
        | EditorPersistenceFailureStage::ReplaceTarget => {
            "project.editor.prefab-open-internal-failed"
        }
    }
}

fn persistence_result_allows_retry(
    result: EditorPersistenceResult,
    request: EditorPersistenceRequestKind,
    document: Option<EditorDocumentId>,
) -> bool {
    match result {
        EditorPersistenceResult::Rejected {
            document: target,
            reason: EditorPersistenceRejection::RequiresReconcile,
            ..
        } => request == EditorPersistenceRequestKind::Reopen && target == document,
        EditorPersistenceResult::Rejected {
            request: failed_request,
            document: failed_document,
            ..
        }
        | EditorPersistenceResult::Failed {
            request: failed_request,
            document: failed_document,
            ..
        } => {
            failed_request == request
                && failed_document.is_none_or(|failed_document| Some(failed_document) == document)
        }
        EditorPersistenceResult::PersistenceUncertain {
            document: uncertain_document,
            ..
        } => {
            request == EditorPersistenceRequestKind::Reopen && Some(uncertain_document) == document
        }
        EditorPersistenceResult::Saved { .. } | EditorPersistenceResult::Opened { .. } => false,
    }
}
