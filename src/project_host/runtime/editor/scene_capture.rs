//! Immutable background composition and owner-thread publication for the selected Scene.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nara_fs::{ContentDigest, FileCapability, FileIdentity, FsError};
use nara_scene::{AuthoringSchemaBasis, SceneAuthoringRevision};
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskReturnState, TaskSpawnRequest, TaskTerminal,
};
use nara_tooling::{EditorDocumentQueryBasis, EditorDocumentQueryRejection};

use crate::project_host::authoring::{
    AuthoringDocumentCapture, AuthoringProjectSourceAdmissionError, AuthoringSourceCaptureInputs,
    AuthoringSourceValidation, CapturedAuthoringPreview,
};

use super::authoring_preview::EditorAuthoringPreviewOmissions;
use super::authoring_sources::AuthoringSourceSelection;
use super::preview::{
    EditorPreviewDependencies, EditorPreviewParticipant, EditorPreviewProjection,
    EditorPreviewSource,
};
use super::*;

const SOURCE_RECHECK: Duration = Duration::from_millis(500);
const CAPTURE_DOMAIN: TaskDomainKey = TaskDomainKey::new(0x7363_656e_6563_6170);

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

struct CaptureSelection {
    root: AuthoringSourceSelection,
    prefabs: Vec<AuthoringSourceSelection>,
    scenes: Vec<AuthoringSourceSelection>,
}

impl CaptureSelection {
    fn checked_retained_bytes(&self) -> Option<usize> {
        let spare = self
            .prefabs
            .capacity()
            .checked_sub(self.prefabs.len())?
            .checked_add(self.scenes.capacity().checked_sub(self.scenes.len())?)?
            .checked_mul(std::mem::size_of::<AuthoringSourceSelection>())?;
        self.prefabs.iter().chain(&self.scenes).try_fold(
            std::mem::size_of::<Self>()
                .checked_add(self.root.checked_retained_bytes()?)?
                .checked_add(spare)?,
            |bytes, source| bytes.checked_add(source.checked_retained_bytes()?),
        )
    }

    fn source(
        &self,
        key: &nara_scene::SourceDocumentKey,
        revision: SceneAuthoringRevision,
    ) -> Option<&AuthoringSourceSelection> {
        self.prefabs.iter().chain(&self.scenes).find(|source| {
            &source.source_key == key && source.revision == revision && source.receipt.is_some()
        })
    }
}

/// A failed admission's retry key, without copying source keys, paths or working documents.
/// It controls scheduling only; accepted captures still compare their complete source facts.
#[derive(PartialEq, Eq)]
struct CaptureRootObservation {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    schema: Option<AuthoringSchemaBasis>,
    source_available: bool,
    persistence: Option<(FileIdentity, Option<FileIdentity>, ContentDigest)>,
}

struct CapturedScene {
    root: AuthoringSourceSelection,
    dependencies: EditorPreviewDependencies,
    validation: AuthoringSourceValidation,
    projection: Option<EditorPreviewProjection>,
    unsaved_dependencies: usize,
    sources: Vec<CapturedSource>,
    source_capture: Option<AuthoringDocumentCapture>,
    native_ready: bool,
    requires_recapture: bool,
    verified: bool,
}

struct CapturedSource {
    key: nara_scene::SourceDocumentKey,
    path: Arc<str>,
    selected: Option<AuthoringSourceSelection>,
}

impl CapturedScene {
    fn matches(&self, editor: &EditorProjectSession) -> bool {
        editor.scene_preview_document == Some(self.root.document)
            && editor.authoring_source_selection_is_current(&self.root)
            && self.sources.iter().all(|source| {
                editor.authoring_dependency_selection_is_current(
                    &source.key,
                    &source.path,
                    source.selected.as_ref(),
                )
            })
    }
}

enum CaptureWork {
    Compose {
        selection: CaptureSelection,
        task: TaskHandle<
            Result<
                AuthoringDocumentCapture,
                crate::project_host::authoring::AuthoringProjectSourceAdmissionError,
            >,
        >,
    },
    Candidate {
        selection: CaptureSelection,
        capture: AuthoringDocumentCapture,
        task: TaskHandle<
            Result<(), crate::project_host::authoring::AuthoringProjectSourceAdmissionError>,
        >,
    },
    Validate(
        TaskHandle<
            Result<(), crate::project_host::authoring::AuthoringProjectSourceAdmissionError>,
        >,
    ),
}

impl CaptureWork {
    fn cancel(&self) {
        match self {
            Self::Compose { task, .. } => {
                task.cancel();
            }
            Self::Candidate { task, .. } => {
                task.cancel();
            }
            Self::Validate(task) => {
                task.cancel();
            }
        }
    }

    fn root(&self) -> Option<&AuthoringSourceSelection> {
        match self {
            Self::Compose { selection, .. } | Self::Candidate { selection, .. } => {
                Some(&selection.root)
            }
            Self::Validate(_) => None,
        }
    }
}

#[derive(Default)]
pub(super) struct EditorSceneCapture {
    generation: u64,
    captured: Option<CapturedScene>,
    pending: Option<CaptureWork>,
    pending_stale: bool,
    failure: Option<DiagnosticReport>,
    failed_attempt: Option<CaptureRootObservation>,
    retry_at: Option<Instant>,
    validate_at: Option<Instant>,
    #[cfg(test)]
    next_completion_gate: Option<Box<dyn FnOnce() + Send>>,
}

impl EditorSceneCapture {
    /// Checks the schema retained by the accepted preview, including its last good capture.
    pub(super) fn source_object_in_use(&self, candidate: &FileCapability) -> Result<bool, FsError> {
        match &self.captured {
            Some(capture) => capture.validation.source_object_in_use(candidate),
            None => Ok(false),
        }
    }

    /// Pending capture work must be fenced and physically returned by the Session first.
    pub(super) fn collect_reclamation_sources(
        &self,
        output: &mut Vec<FileCapability>,
    ) -> Result<(), FsError> {
        if self.pending.is_some() {
            return Err(FsError::Unproven {
                operation: nara_fs::FsOperation::InspectHandle,
                proof: "scene capture work has not physically returned",
            });
        }
        if let Some(capture) = &self.captured {
            capture.validation.collect_reclamation_sources(output)?;
        }
        Ok(())
    }

    /// Published preview packets do not block adoption; admitted capture work does.
    pub(super) fn schema_adoption_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Revokes pending capture publication and observes return without retrying composition.
    /// The accepted preview remains available until the adoption turn invalidates it.
    pub(super) fn observe_schema_adoption_drain(&mut self) {
        let Some(work) = &self.pending else {
            return;
        };
        work.cancel();
        self.pending_stale = true;
        let state = match work {
            CaptureWork::Compose { task, .. } => task.return_observer().state(),
            CaptureWork::Candidate { task, .. } => task.return_observer().state(),
            CaptureWork::Validate(task) => task.return_observer().state(),
        };
        if state != TaskReturnState::Pending {
            self.pending = None;
            self.pending_stale = false;
            self.validate_at = Some(Instant::now());
        }
    }

    pub(super) fn cancel(&mut self) {
        if let Some(work) = &self.pending {
            work.cancel();
        }
        self.pending_stale = self.pending.is_some();
        self.captured = None;
        self.failed_attempt = None;
        self.failure = None;
        self.retry_at = None;
        self.validate_at = None;
    }

    pub(super) fn invalidate(&mut self) {
        if let Some(work) = &self.pending {
            work.cancel();
        }
        self.pending_stale = self.pending.is_some();
        self.captured = None;
        self.failure = None;
        self.failed_attempt = None;
        self.retry_at = None;
        self.validate_at = None;
    }

    fn next_generation(&mut self) -> bool {
        let Some(next) = self.generation.checked_add(1) else {
            self.failure = Some(capture_error(
                "project.editor.scene-capture-generation-exhausted",
            ));
            return false;
        };
        self.generation = next;
        true
    }

    pub(super) fn has_pending_work(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn recheck_after(&self) -> Option<Duration> {
        self.retry_at
            .or(self.validate_at)
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    pub(super) fn unsaved_dependencies(&self) -> usize {
        self.captured
            .as_ref()
            .map_or(0, |capture| capture.unsaved_dependencies)
    }
}

impl EditorProjectSession {
    /// Polls only admitted Scene capture while the ordinary preview driver is fenced.
    pub(super) fn drain_schema_adoption_scene_capture(&mut self) {
        self.scene_capture.observe_schema_adoption_drain();
    }

    fn capture_selection(
        &self,
    ) -> Result<
        Option<(CaptureSelection, AuthoringSourceCaptureInputs)>,
        AuthoringProjectSourceAdmissionError,
    > {
        let Some(document) = self
            .scene_preview_document
            .filter(|document| self.workspace.scene(*document).is_some())
        else {
            return Ok(None);
        };
        let overflow = || AuthoringProjectSourceAdmissionError::AccountingOverflow;
        let root_bytes = self
            .authoring_source_selection_retained_bytes(document)
            .ok_or_else(overflow)?;
        let selection_bytes = self
            .scene_lanes
            .keys()
            .filter(|&&other| other != document)
            .chain(self.prefab_lanes.keys())
            .try_fold(root_bytes, |bytes, &document| {
                bytes.checked_add(self.authoring_source_selection_retained_bytes(document)?)
            })
            .ok_or_else(overflow)?;
        // Keep Host observations distinct from the snapshots released by the worker.
        let working_bytes = self.scene_working_set_retained_bytes(document)?;
        let root_source_bytes = self
            .scene_lanes
            .get(&document)
            .and_then(|lane| lane.authoring_source.as_ref())
            .map_or(Some(0), |source| source.checked_retained_bytes())
            .ok_or_else(overflow)?;
        let host_retained = selection_bytes
            .checked_add(std::mem::size_of::<CaptureSelection>())
            .ok_or_else(overflow)?;
        let worker_retained = working_bytes
            .checked_add(root_source_bytes)
            .ok_or_else(overflow)?;
        let admitted = self
            .authoring_project()
            .expect("authoring capture owns its project")
            .admit_source_capture_inputs(host_retained, worker_retained)?;
        Ok(Some((
            CaptureSelection {
                root: self
                    .authoring_source_selection(document)
                    .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?,
                prefabs: self.authoring_prefab_selections(),
                scenes: self.authoring_scene_selections(document),
            },
            admitted,
        )))
    }

    fn capture_root_is_current(&self, root: &AuthoringSourceSelection) -> bool {
        self.scene_preview_document == Some(root.document)
            && self.authoring_source_selection_is_current(root)
    }

    fn capture_root_observation(&self) -> Option<CaptureRootObservation> {
        let document = self.scene_preview_document?;
        let slot = self.workspace.scene(document)?;
        let lane = self.scene_lanes.get(&document);
        Some(CaptureRootObservation {
            document,
            revision: slot.revision(),
            schema: slot.session().authoring_schema_basis(),
            source_available: lane.is_some_and(|lane| lane.authoring_source.is_some()),
            persistence: lane.and_then(|lane| {
                lane.host
                    .try_with(|host| host.accepted_source_observation())
                    .flatten()
            }),
        })
    }

    fn start_current_scene_capture(&mut self) {
        if self.scene_capture.pending.is_none() {
            match self.capture_selection() {
                Ok(Some((selection, inputs))) => self.start_scene_capture(selection, inputs),
                Ok(None) => {}
                Err(error) => {
                    self.reject_scene_capture(admission_error(error));
                }
            }
        }
    }

    pub(super) fn initialize_authoring_scene_capture(&mut self) {
        let captured = match self.capture_startup_scene_observations() {
            Ok(Some(captured)) => captured,
            Ok(None) => return,
            Err(error) => {
                self.reject_scene_capture(admission_error(error));
                return;
            }
        };
        self.scene_capture.generation = 1;
        self.scene_capture.captured = Some(captured);
        self.scene_capture.validate_at = Some(Instant::now());
    }

    fn capture_startup_scene_observations(
        &mut self,
    ) -> Result<Option<CapturedScene>, AuthoringProjectSourceAdmissionError> {
        let Some(document) = self
            .scene_preview_document
            .filter(|document| self.workspace.scene(*document).is_some())
        else {
            return Ok(None);
        };
        let root_bytes = self
            .authoring_source_selection_retained_bytes(document)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let Some(project) = self.authoring_project() else {
            return Ok(None);
        };
        project
            .startup_source_validation()
            .with_additional_retained_bytes(root_bytes)?;
        let selection = CaptureSelection {
            root: self
                .authoring_source_selection(document)
                .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?,
            prefabs: Vec::new(),
            scenes: Vec::new(),
        };
        let observed = || {
            project
                .prefab_sources()
                .map(|prefab| {
                    (
                        prefab.native_source().key(),
                        prefab.source().project_path().as_str(),
                        None,
                    )
                })
                .chain(
                    project
                        .scene_sources()
                        .iter()
                        .filter(|scene| scene.native_source().key() != &selection.root.source_key)
                        .map(|scene| {
                            (
                                scene.native_source().key(),
                                scene.source().project_path().as_str(),
                                None,
                            )
                        }),
                )
        };
        let cost = observation_cost(&selection, observed(), 0)?;
        let validation = project
            .startup_source_validation()
            .with_additional_retained_bytes(cost.retained_bytes)?;
        let mut sources = Vec::with_capacity(cost.source_count);
        for (key, path, _) in observed() {
            sources.push(CapturedSource {
                key: key.clone(),
                path: Arc::from(path),
                selected: None,
            });
        }
        let database = project.asset_database();
        let asset_scope = project.asset_scope();
        let Some(preview) = self
            .authoring_project_mut()
            .expect("startup capture owns its project")
            .take_startup_preview()
        else {
            return Ok(None);
        };
        Ok(Some(CapturedScene {
            projection: Some(projection(
                selection.root.document,
                selection.root.revision,
                preview,
                database,
                asset_scope,
            )),
            root: selection.root,
            dependencies: EditorPreviewDependencies::captured(1, Arc::from([])),
            validation,
            unsaved_dependencies: 0,
            sources,
            source_capture: None,
            native_ready: true,
            requires_recapture: false,
            verified: false,
        }))
    }

    fn start_scene_capture(
        &mut self,
        selection: CaptureSelection,
        inputs: AuthoringSourceCaptureInputs,
    ) {
        if self.scene_capture.pending.is_some() {
            return;
        }
        if !self.scene_capture.next_generation() {
            return;
        }
        self.authoring_runtime.invalidate_preview_content();
        // Release the predecessor before another budgeted capture owns its sources.
        self.scene_capture.captured = None;
        self.scene_capture.failure = None;
        self.scene_capture.failed_attempt = None;
        self.scene_capture.retry_at = None;
        self.scene_capture.validate_at = None;
        let Some(root_source) = selection.root.receipt.as_ref() else {
            self.reject_scene_capture(capture_error(
                "project.editor.scene-capture-source-unavailable",
            ));
            return;
        };
        let open_prefabs = self.capture_open_prefabs(&selection.prefabs);
        let open_scenes = self.capture_open_scenes(&selection.scenes);
        let request = self
            .authoring_project()
            .expect("authoring capture has a project")
            .capture_scene_request(
                root_source,
                self.workspace
                    .scene(selection.root.document)
                    .expect("captured Scene is open")
                    .session(),
                inputs,
                open_prefabs,
            )
            .map(|request| request.with_open_scenes(open_scenes));
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                self.reject_scene_capture(admission_error(error));
                return;
            }
        };
        #[cfg(test)]
        let gate = self.scene_capture.next_completion_gate.take();
        let task = self
            .authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(self.scene_capture.generation, CAPTURE_DOMAIN),
                move |_| {
                    let prepared = request.prepare();
                    #[cfg(test)]
                    if let Some(gate) = gate {
                        gate();
                    }
                    prepared
                },
            )
            .into_handle()
            .ok();
        match task {
            Some(task) => {
                self.scene_capture.pending = Some(CaptureWork::Compose { selection, task })
            }
            None => self
                .reject_scene_capture(capture_error("project.editor.scene-capture-task-rejected")),
        }
    }

    fn reject_scene_capture(&mut self, diagnostics: DiagnosticReport) {
        self.authoring_runtime.invalidate_preview_content();
        self.scene_capture.captured = None;
        self.scene_capture.pending = None;
        self.scene_capture.failed_attempt = self.capture_root_observation();
        if !self.scene_capture.next_generation() {
            return;
        }
        self.scene_capture.failure = Some(diagnostics);
        self.scene_capture.retry_at = Some(Instant::now() + SOURCE_RECHECK);
    }

    fn validate_scene_candidate(
        &mut self,
        selection: CaptureSelection,
        capture: AuthoringDocumentCapture,
    ) {
        let validation = capture.source_validation();
        let task = self
            .authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(self.scene_capture.generation, CAPTURE_DOMAIN),
                move |_| validation.validate(),
            )
            .into_handle()
            .ok();
        match task {
            Some(task) => {
                self.scene_capture.pending = Some(CaptureWork::Candidate {
                    selection,
                    capture,
                    task,
                })
            }
            None => self.reject_scene_capture(capture_error(
                "project.editor.scene-validation-task-rejected",
            )),
        }
    }

    fn accept_scene_capture(
        &mut self,
        selection: CaptureSelection,
        mut capture: AuthoringDocumentCapture,
    ) {
        if capture.root_revision() != Some(selection.root.revision) {
            self.reject_scene_capture(capture_error(
                "project.editor.scene-capture-revision-mismatch",
            ));
            return;
        }
        let cost = match observation_cost(
            &selection,
            demanded_sources(&capture, &selection.root.source_key),
            capture.diagnostics().retained_bytes(),
        ) {
            Ok(cost) => cost,
            Err(error) => {
                self.reject_scene_capture(admission_error(error));
                return;
            }
        };
        let overlap = selection
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(cost.retained_bytes))
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
            .and_then(|bytes| capture.replace_host_retained_bytes(bytes));
        if let Err(error) = overlap {
            self.reject_scene_capture(admission_error(error));
            return;
        }
        let requires_recapture = capture.requires_recapture();
        let preview = capture.take_preview();
        let native_ready = preview.is_some();
        let projection = Some(match preview {
            Some(preview) => projection(
                selection.root.document,
                selection.root.revision,
                preview,
                capture.asset_database(),
                capture.asset_scope(),
            ),
            None => EditorPreviewProjection::rejected(capture.diagnostics().clone()),
        });
        let mut participants = Vec::with_capacity(cost.participant_count);
        let mut unsaved = 0;
        let mut sources = Vec::with_capacity(cost.source_count);
        let CaptureSelection {
            root,
            mut prefabs,
            mut scenes,
        } = selection;
        for (key, path, revision) in demanded_sources(&capture, &root.source_key) {
            let Some(revision) = revision else {
                sources.push(CapturedSource {
                    key: key.clone(),
                    path: Arc::from(path),
                    selected: None,
                });
                continue;
            };
            let offered = match key.kind() {
                nara_scene::SourceDocumentKind::Scene => &mut scenes,
                nara_scene::SourceDocumentKind::Prefab => &mut prefabs,
            };
            let Some(source) = offered
                .iter()
                .position(|source| {
                    &source.source_key == key
                        && source.revision == revision
                        && source.receipt.is_some()
                })
                .map(|index| offered.swap_remove(index))
            else {
                self.reject_scene_capture(capture_error(
                    "project.editor.scene-capture-participant-missing",
                ));
                return;
            };
            let document = source.document;
            sources.push(CapturedSource {
                key: key.clone(),
                path: Arc::from(path),
                selected: Some(source),
            });
            participants.push(EditorPreviewParticipant {
                document,
                revision,
                source_key: key.clone(),
            });
            unsaved += usize::from(self.workspace.document_is_dirty(document) == Some(true));
        }
        drop((prefabs, scenes));
        if let Err(error) = capture.replace_host_retained_bytes(cost.retained_bytes) {
            self.reject_scene_capture(admission_error(error));
            return;
        }
        let validation = capture.source_validation();
        if !self.scene_capture.next_generation() {
            return;
        }
        let dependencies =
            EditorPreviewDependencies::captured(self.scene_capture.generation, participants.into());
        let prepared = CapturedScene {
            projection,
            root,
            dependencies,
            validation,
            unsaved_dependencies: unsaved,
            sources,
            source_capture: Some(capture),
            native_ready,
            requires_recapture,
            verified: true,
        };
        if !prepared.matches(self) {
            self.scene_capture.invalidate();
            return;
        }
        if native_ready && let EditorExecutionLane::Authoring(project) = &self.execution {
            self.image_import.enqueue_authoring_capture(
                project,
                prepared
                    .source_capture
                    .as_ref()
                    .expect("prepared source capture is retained"),
            );
        }
        self.scene_capture.captured = Some(prepared);
        self.scene_capture.failure = None;
        self.scene_capture.failed_attempt = None;
        self.scene_capture.retry_at = None;
        self.scene_capture.validate_at = Some(Instant::now() + SOURCE_RECHECK);
    }

    fn poll_scene_capture(&mut self) {
        let Some(work) = self.scene_capture.pending.take() else {
            return;
        };
        match work {
            CaptureWork::Compose {
                selection: original,
                mut task,
            } => {
                if task.return_observer().state() == TaskReturnState::Pending {
                    self.scene_capture.pending = Some(CaptureWork::Compose {
                        selection: original,
                        task,
                    });
                    return;
                }
                if self.scene_capture.pending_stale {
                    self.scene_capture.pending_stale = false;
                    return;
                }
                match task.try_take() {
                    Some(TaskTerminal::Completed(Ok(capture))) => {
                        if !self.capture_root_is_current(&original.root) {
                            self.scene_capture.invalidate();
                        } else {
                            self.validate_scene_candidate(original, capture);
                        }
                    }
                    Some(TaskTerminal::Completed(Err(error))) => {
                        self.reject_scene_capture(admission_error(error))
                    }
                    _ => self.reject_scene_capture(capture_error(
                        "project.editor.scene-capture-task-failed",
                    )),
                }
            }
            CaptureWork::Candidate {
                selection: original,
                capture,
                mut task,
            } => {
                if task.return_observer().state() == TaskReturnState::Pending {
                    self.scene_capture.pending = Some(CaptureWork::Candidate {
                        selection: original,
                        capture,
                        task,
                    });
                    return;
                }
                if self.scene_capture.pending_stale {
                    self.scene_capture.pending_stale = false;
                    return;
                }
                match task.try_take() {
                    Some(TaskTerminal::Completed(Ok(()))) => {
                        if self.capture_root_is_current(&original.root) {
                            self.accept_scene_capture(original, capture);
                        } else {
                            self.scene_capture.invalidate();
                        }
                    }
                    Some(TaskTerminal::Completed(Err(error))) => {
                        use crate::project_host::authoring::AuthoringProjectSourceAdmissionError as E;
                        if matches!(&error, E::Budget(_) | E::AccountingOverflow) {
                            self.reject_scene_capture(admission_error(error));
                        } else {
                            self.scene_capture.invalidate();
                            self.authoring_runtime.invalidate_preview_content();
                            self.start_current_scene_capture();
                        }
                    }
                    _ => self.reject_scene_capture(capture_error(
                        "project.editor.scene-validation-task-failed",
                    )),
                }
            }
            CaptureWork::Validate(mut task) => {
                if task.return_observer().state() == TaskReturnState::Pending {
                    self.scene_capture.pending = Some(CaptureWork::Validate(task));
                    return;
                }
                if self.scene_capture.pending_stale {
                    self.scene_capture.pending_stale = false;
                    return;
                }
                match task.try_take() {
                    Some(TaskTerminal::Completed(Ok(()))) => {
                        if self
                            .scene_capture
                            .captured
                            .as_ref()
                            .is_some_and(|capture| !capture.verified)
                        {
                            if !self.scene_capture.next_generation() {
                                return;
                            }
                            let generation = self.scene_capture.generation;
                            let capture = self
                                .scene_capture
                                .captured
                                .as_mut()
                                .expect("startup validation retains its candidate");
                            capture.dependencies = dependencies(generation, &capture.sources);
                            capture.verified = true;
                        }
                        self.scene_capture.validate_at = Some(Instant::now() + SOURCE_RECHECK);
                    }
                    _ => {
                        self.scene_capture.invalidate();
                        self.start_current_scene_capture();
                    }
                }
            }
        }
    }

    /// Keeps task return observable after its presentation consumer becomes unavailable.
    pub(super) fn cancel_authoring_scene_capture(&mut self) {
        self.scene_capture.cancel();
        self.poll_scene_capture();
        self.authoring_runtime.invalidate_preview_content();
    }

    pub(super) fn drive_authoring_scene_capture(&mut self) -> Option<EditorPreviewSource> {
        let Some(document) = self
            .scene_preview_document
            .filter(|document| self.workspace.scene(*document).is_some())
        else {
            self.cancel_authoring_scene_capture();
            return None;
        };
        let current = self.scene_capture.captured.as_ref().map_or_else(
            || {
                self.scene_capture.failed_attempt.as_ref().map_or_else(
                    || {
                        self.scene_capture
                            .pending
                            .as_ref()
                            .and_then(CaptureWork::root)
                            .is_some_and(|root| self.capture_root_is_current(root))
                    },
                    |attempt| self.capture_root_observation().as_ref() == Some(attempt),
                )
            },
            |capture| capture.matches(self),
        );
        if !current {
            self.scene_capture.invalidate();
            self.authoring_runtime.invalidate_preview_content();
            self.poll_scene_capture();
            self.start_current_scene_capture();
        } else {
            self.poll_scene_capture();
        }
        let now = Instant::now();
        if self.scene_capture.pending.is_none() {
            if self.scene_capture.captured.is_none()
                && self
                    .scene_capture
                    .retry_at
                    .is_none_or(|deadline| deadline <= now)
            {
                self.start_current_scene_capture();
            } else if self
                .scene_capture
                .validate_at
                .is_some_and(|deadline| deadline <= now)
                && let Some(capture) = &self.scene_capture.captured
            {
                if capture.requires_recapture {
                    // Successful receipts cannot observe a previously missing dependency.
                    self.start_current_scene_capture();
                } else {
                    let validation = capture.validation.clone();
                    self.scene_capture.validate_at = None;
                    match self
                        .authoring_runtime
                        .persistence_tasks()
                        .spawn(
                            TaskPoolKind::Io,
                            TaskSpawnRequest::new(self.scene_capture.generation, CAPTURE_DOMAIN),
                            move |_| validation.validate(),
                        )
                        .into_handle()
                        .ok()
                    {
                        Some(task) => {
                            self.scene_capture.pending = Some(CaptureWork::Validate(task))
                        }
                        None => self.reject_scene_capture(capture_error(
                            "project.editor.scene-validation-task-rejected",
                        )),
                    }
                }
            }
        }
        let (dependencies, projection) = if let Some(capture) = self
            .scene_capture
            .captured
            .as_mut()
            .filter(|capture| capture.verified)
        {
            (capture.dependencies.clone(), capture.projection.take())
        } else {
            let diagnostics = self.scene_capture.failure.clone().unwrap_or_default();
            (
                EditorPreviewDependencies::captured(self.scene_capture.generation, Arc::from([])),
                Some(EditorPreviewProjection::rejected(diagnostics)),
            )
        };
        Some(EditorPreviewSource {
            document,
            revision: self
                .workspace
                .scene(document)
                .expect("selected Scene remains open")
                .revision(),
            dependencies,
            projection,
        })
    }

    /// Reads the first typed composition failure for the selected Scene's verified capture.
    ///
    /// `None` means no current typed failure is available, including while capture is pending
    /// or the queried document is not the selected Scene. It never proves successful composition
    /// or native activation. This query neither reads files nor drives preparation.
    pub fn prefab_composition_failure(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<Option<&nara_scene::PrefabExpansionFailure>, EditorDocumentQueryRejection> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorDocumentQueryRejection::Closing);
        }
        self.workspace.validate_document_query_basis(basis)?;
        let Some(capture) = self.scene_capture.captured.as_ref().filter(|capture| {
            capture.verified
                && capture.root.document == basis.document
                && capture.root.revision == basis.revision
                && capture.root.schema == basis.schema
        }) else {
            return Ok(None);
        };
        if !capture.matches(self) {
            return Ok(None);
        }
        Ok(capture
            .source_capture
            .as_ref()
            .and_then(AuthoringDocumentCapture::composition_failure))
    }

    /// Reads only accepted owner state; it neither reads files nor drives preparation.
    #[doc(hidden)]
    pub fn current_editor_preview_content_stamp(&self) -> Option<EditorPreviewContentStamp> {
        if self.closing {
            return None;
        }
        if self.authoring_project().is_some() {
            let capture = self.scene_capture.captured.as_ref()?;
            if !capture.verified || !capture.native_ready {
                return None;
            }
            if !capture.matches(self) {
                return None;
            }
        } else {
            let document = self.scene_preview_document?;
            let revision = self.workspace.scene(document)?.revision();
            if self.authoring_runtime.preview_requires_source_projection(
                document,
                revision,
                &EditorPreviewDependencies::empty(),
            ) {
                return None;
            }
        }
        self.authoring_runtime.current_preview_content_stamp()
    }
}

fn projection(
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    preview: CapturedAuthoringPreview,
    database: Arc<nara_asset::ProjectAssetDatabase>,
    asset_scope: Arc<nara_asset::AssetPathScope>,
) -> EditorPreviewProjection {
    EditorPreviewProjection::authoring(
        preview.candidate,
        EditorAuthoringPreviewOmissions::new(document, revision, preview.omissions),
        database,
        asset_scope,
    )
}

fn dependencies(generation: u64, sources: &[CapturedSource]) -> EditorPreviewDependencies {
    EditorPreviewDependencies::captured(
        generation,
        sources
            .iter()
            .filter_map(|observed| {
                let source = observed.selected.as_ref()?;
                Some(EditorPreviewParticipant {
                    document: source.document,
                    revision: source.revision,
                    source_key: observed.key.clone(),
                })
            })
            .collect::<Vec<_>>()
            .into(),
    )
}

fn demanded_sources<'a>(
    capture: &'a AuthoringDocumentCapture,
    root: &'a nara_scene::SourceDocumentKey,
) -> impl Iterator<
    Item = (
        &'a nara_scene::SourceDocumentKey,
        &'a str,
        Option<SceneAuthoringRevision>,
    ),
> + 'a {
    capture
        .prefabs()
        .iter()
        .map(|prefab| {
            (
                prefab.native_source().key(),
                prefab.source().project_path().as_str(),
                prefab.session_revision(),
            )
        })
        .chain(
            capture
                .scenes()
                .iter()
                .filter(move |scene| scene.native_source().key() != root)
                .map(|scene| {
                    (
                        scene.native_source().key(),
                        scene.source().project_path().as_str(),
                        scene.session_revision(),
                    )
                }),
        )
}

struct CaptureObservationCost {
    retained_bytes: usize,
    source_count: usize,
    participant_count: usize,
}

fn observation_cost<'a>(
    selection: &CaptureSelection,
    sources: impl Iterator<
        Item = (
            &'a nara_scene::SourceDocumentKey,
            &'a str,
            Option<SceneAuthoringRevision>,
        ),
    >,
    diagnostics_bytes: usize,
) -> Result<CaptureObservationCost, AuthoringProjectSourceAdmissionError> {
    let overflow = || AuthoringProjectSourceAdmissionError::AccountingOverflow;
    let mut cost = CaptureObservationCost {
        retained_bytes: std::mem::size_of::<CapturedScene>()
            .checked_add(
                selection
                    .root
                    .checked_retained_bytes()
                    .ok_or_else(overflow)?,
            )
            .and_then(|bytes| bytes.checked_add(2 * std::mem::size_of::<usize>()))
            .and_then(|bytes| bytes.checked_add(diagnostics_bytes))
            .ok_or_else(overflow)?,
        source_count: 0,
        participant_count: 0,
    };
    for (key, path, revision) in sources {
        let mut bytes = std::mem::size_of::<CapturedSource>()
            .checked_add(key.retained_bytes())
            .and_then(|bytes| bytes.checked_add(path.len()))
            .and_then(|bytes| bytes.checked_add(2 * std::mem::size_of::<usize>()))
            .ok_or_else(overflow)?;
        if let Some(revision) = revision {
            let selected = selection
                .source(key, revision)
                .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?;
            bytes = bytes
                .checked_add(selected.checked_retained_bytes().ok_or_else(overflow)?)
                // The temporary participant vector and final Arc overlap, but move their keys.
                .and_then(|bytes| {
                    bytes.checked_add(2 * std::mem::size_of::<EditorPreviewParticipant>())
                })
                .and_then(|bytes| bytes.checked_add(key.retained_bytes()))
                .ok_or_else(overflow)?;
            cost.participant_count = cost.participant_count.checked_add(1).ok_or_else(overflow)?;
        }
        cost.retained_bytes = cost
            .retained_bytes
            .checked_add(bytes)
            .ok_or_else(overflow)?;
        cost.source_count = cost.source_count.checked_add(1).ok_or_else(overflow)?;
    }
    Ok(cost)
}

fn capture_error(code: &'static str) -> DiagnosticReport {
    single_error(
        code,
        "The selected Scene preview could not capture its current dependencies",
    )
}

fn admission_error(
    error: crate::project_host::authoring::AuthoringProjectSourceAdmissionError,
) -> DiagnosticReport {
    use crate::project_host::authoring::AuthoringProjectSourceAdmissionError as E;
    match error {
        E::CompilationBudget(report) => report,
        E::SourcePublication { source, .. } => source.into_diagnostics(),
        E::SourceStructure { source, .. } => source.into_diagnostics(),
        E::NativeSource(error) => {
            let mut report = DiagnosticReport::default();
            report.push(error.diagnostic());
            report
        }
        E::SchemaBasisMismatch => capture_error("project.editor.scene-capture-schema-mismatch"),
        E::DocumentSourceMismatch => capture_error("project.editor.scene-capture-source-mismatch"),
        E::AmbiguousPrefabSession { path } => capture_source_error(
            "project.editor.scene-capture-source-ambiguous",
            path.as_str(),
        ),
        E::SourceChanged { path, .. } => {
            capture_source_error("project.editor.scene-capture-source-changed", path.as_str())
        }
        E::SourceFormat { path, source, .. } => {
            let mut report = DiagnosticReport::default();
            report.push(with_source(
                crate::scene_format_diagnostic::scene_format_diagnostic(
                    "project.editor.scene-capture-source-format-invalid",
                    "A selected Scene dependency has an invalid source format",
                    &source,
                ),
                path.as_str(),
            ));
            report
        }
        E::InputFormat { path, .. } => {
            capture_source_error("project.editor.scene-capture-input-rejected", path.as_str())
        }
        E::SourcePath { path, .. }
        | E::SourceHost { path, .. }
        | E::SourceIdentityKindConflict { path }
        | E::SourceTooLarge { path, .. }
        | E::UnsupportedSourceEncoding { path, .. }
        | E::DocumentSourcePath { path, .. } => capture_source_error(
            "project.editor.scene-capture-source-rejected",
            path.as_str(),
        ),
        E::AssetMetadataChanged { path }
        | E::AssetMetadataFormat { path, .. }
        | E::IntrinsicSourceMetadataMismatch { path }
        | E::AssetIdentity { path, .. } => {
            capture_source_error("project.editor.scene-capture-asset-rejected", path.as_str())
        }
        E::AssetMetadataPathMismatch { requested, .. } => capture_source_error(
            "project.editor.scene-capture-asset-path-mismatch",
            requested.as_str(),
        ),
        E::AccountingOverflow => capture_error("project.editor.scene-capture-accounting-overflow"),
        E::Budget(error) => budget_error(error),
        E::MissingStartupScene
        | E::SessionSnapshotPublication
        | E::SourceDirectoryPath { .. }
        | E::SourceDirectoryHost { .. }
        | E::JoinedSourcePath(_)
        | E::CatalogueNameInvalid
        | E::AssetDirectoryAuthorityUnavailable => {
            capture_error("project.editor.scene-capture-admission-rejected")
        }
    }
}

fn capture_source_error(code: &'static str, path: &str) -> DiagnosticReport {
    let diagnostic = nara_diagnostic::Diagnostic::error(
        super::super::diagnostic_code(code),
        super::super::safe_summary("A selected Scene dependency could not be admitted"),
    );
    let mut report = DiagnosticReport::default();
    report.push(with_source(diagnostic, path));
    report
}

fn with_source(diagnostic: nara_diagnostic::Diagnostic, path: &str) -> nara_diagnostic::Diagnostic {
    let Ok(field) =
        nara_diagnostic::DiagnosticField::project_relative(super::super::field_key("source"), path)
    else {
        return diagnostic;
    };
    diagnostic
        .clone()
        .try_with_field(field)
        .unwrap_or(diagnostic)
}

fn budget_error(
    error: crate::project_host::authoring::AuthoringProjectAdmissionError,
) -> DiagnosticReport {
    use crate::project_host::authoring::AuthoringProjectAdmissionError as E;
    let (kind, maximum) = match error {
        E::AggregateEncodedBytesExceeded { maximum } => ("encoded-bytes", maximum),
        E::UniqueFileLimitExceeded { maximum } => ("files", maximum),
        E::DependencyEdgeLimitExceeded { maximum } => ("dependencies", maximum),
        E::TraversalDepthExceeded { maximum } => ("depth", maximum),
        E::AggregatePathBytesExceeded { maximum } => ("path-bytes", maximum),
        E::PeakOpenHandlesExceeded { maximum } => ("open-handles", maximum),
        E::AggregateDecodedBytesExceeded { maximum } => ("decoded-bytes", maximum),
        E::DecodeWorkExceeded { maximum } => ("work", maximum),
        E::DiagnosticItemLimitExceeded { maximum } => ("diagnostics", maximum),
        E::DiagnosticBytesExceeded { maximum } => ("diagnostic-bytes", maximum),
        E::RetainedCandidateBytesExceeded { maximum } => ("retained-bytes", maximum),
        _ => return capture_error("project.editor.scene-capture-admission-rejected"),
    };
    let diagnostic = nara_diagnostic::Diagnostic::error(
        super::super::diagnostic_code("project.editor.scene-capture-budget-exceeded"),
        super::super::safe_summary("The selected Scene capture exceeds its preparation budget"),
    )
    .try_with_field(nara_diagnostic::DiagnosticField::public_identifier(
        super::super::field_key("budget"),
        nara_diagnostic::PublicDiagnosticIdentifier::new(kind)
            .expect("budget names are engine-owned"),
    ))
    .and_then(|diagnostic| {
        diagnostic.try_with_field(nara_diagnostic::DiagnosticField::public_u64(
            super::super::field_key("maximum"),
            u64::try_from(maximum).unwrap_or(u64::MAX),
        ))
    })
    .expect("capture budget fields are distinct");
    let mut report = DiagnosticReport::default();
    report.push(diagnostic);
    report
}
