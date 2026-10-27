//! Immutable background composition and owner-thread publication for the selected Scene.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nara_scene::SceneAuthoringRevision;
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskReturnState, TaskSpawnRequest, TaskTerminal,
};
use nara_tooling::{EditorDocumentQueryBasis, EditorDocumentQueryRejection};

use crate::project_host::authoring::{
    AuthoringDocumentCapture, AuthoringPrefabSessionSnapshot, AuthoringSourceValidation,
    CapturedAuthoringPreview, document::AuthoringDocumentSourceFact,
};

use super::authoring_preview::EditorAuthoringPreviewOmissions;
use super::preview::{
    EditorPreviewDependencies, EditorPreviewParticipant, EditorPreviewProjection,
    EditorPreviewSource,
};
use super::*;

const SOURCE_RECHECK: Duration = Duration::from_millis(500);
const CAPTURE_DOMAIN: TaskDomainKey = TaskDomainKey::new(0x7363_656e_6563_6170);

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

#[derive(Clone, PartialEq, Eq)]
struct SourceSelection {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    schema: Option<nara_scene::AuthoringSchemaBasis>,
    source_key: nara_scene::SourceDocumentKey,
    receipt: Option<AuthoringDocumentSourceFact>,
}

#[derive(Clone, PartialEq, Eq)]
struct CaptureSelection {
    root: SourceSelection,
    prefabs: Vec<SourceSelection>,
}

struct CapturedScene {
    selection: CaptureSelection,
    dependencies: EditorPreviewDependencies,
    validation: AuthoringSourceValidation,
    projection: Option<EditorPreviewProjection>,
    unsaved_dependencies: usize,
    sources: Vec<(nara_scene::SourceDocumentKey, Option<SourceSelection>)>,
    source_capture: Option<AuthoringDocumentCapture>,
    native_ready: bool,
    requires_recapture: bool,
    verified: bool,
}

impl CapturedScene {
    fn matches(&self, current: &CaptureSelection) -> bool {
        self.selection.root == current.root
            && self.sources.iter().all(|(key, expected)| {
                current
                    .prefabs
                    .iter()
                    .find(|source| &source.source_key == key)
                    == expected.as_ref()
            })
    }
}

enum CaptureWork {
    Compose(
        TaskHandle<
            Result<
                AuthoringDocumentCapture,
                crate::project_host::authoring::AuthoringProjectSourceAdmissionError,
            >,
        >,
    ),
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
            Self::Compose(task) => {
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
}

#[derive(Default)]
pub(super) struct EditorSceneCapture {
    generation: u64,
    selection: Option<CaptureSelection>,
    captured: Option<CapturedScene>,
    pending: Option<CaptureWork>,
    pending_stale: bool,
    failure: Option<DiagnosticReport>,
    retry_at: Option<Instant>,
    validate_at: Option<Instant>,
    #[cfg(test)]
    next_completion_gate: Option<Box<dyn FnOnce() + Send>>,
}

impl EditorSceneCapture {
    pub(super) fn cancel(&mut self) {
        if let Some(work) = &self.pending {
            work.cancel();
        }
        self.pending_stale = self.pending.is_some();
        self.captured = None;
        self.selection = None;
    }

    pub(super) fn invalidate(&mut self) {
        if let Some(work) = &self.pending {
            work.cancel();
        }
        self.pending_stale = self.pending.is_some();
        self.captured = None;
        self.failure = None;
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
    fn capture_selection(&self) -> Option<CaptureSelection> {
        let document = self.scene_preview_document?;
        let slot = self.workspace.scene(document)?;
        let lane = self.scene_lanes.get(&document)?;
        let receipt = lane.authoring_source.as_ref().and_then(|source| {
            let (identity, parent, digest) = lane
                .host
                .try_with(|host| host.accepted_source_observation())??;
            source
                .with_persistence_observation(identity, parent, digest)
                .ok()
        });
        let root = SourceSelection {
            document,
            revision: slot.revision(),
            schema: slot.session().authoring_schema_basis(),
            source_key: lane.source_key.clone(),
            receipt,
        };
        let prefabs = self
            .prefab_lanes
            .iter()
            .filter_map(|(&document, lane)| {
                let slot = self.workspace.prefab(document)?;
                let receipt = lane.authoring_source.as_ref().and_then(|source| {
                    lane.host
                        .try_with(|host| host.accepted_source_observation())
                        .flatten()
                        .and_then(|(identity, parent, digest)| {
                            source
                                .with_persistence_observation(identity, parent, digest)
                                .ok()
                        })
                });
                Some(SourceSelection {
                    document,
                    revision: slot.revision(),
                    schema: slot.session().authoring_schema_basis(),
                    source_key: lane.source_key.clone(),
                    receipt,
                })
            })
            .collect();
        Some(CaptureSelection { root, prefabs })
    }

    pub(super) fn initialize_authoring_scene_capture(&mut self) {
        let Some(selection) = self.capture_selection() else {
            return;
        };
        let Some(project) = self.authoring_project_mut() else {
            return;
        };
        let validation = project.startup_source_validation();
        let database = project.asset_database();
        let sources = project
            .prefab_sources()
            .map(|prefab| (prefab.native_source().key().clone(), None))
            .collect();
        let preview = project.take_startup_preview();
        let dependencies = EditorPreviewDependencies::captured(1, Arc::from([]));
        self.scene_capture.generation = 1;
        self.scene_capture.selection = Some(selection.clone());
        if let Some(preview) = preview {
            self.scene_capture.captured = Some(CapturedScene {
                projection: Some(projection(
                    selection.root.document,
                    selection.root.revision,
                    preview,
                    database,
                )),
                dependencies,
                selection,
                validation,
                unsaved_dependencies: 0,
                sources,
                source_capture: None,
                native_ready: true,
                requires_recapture: false,
                verified: false,
            });
            self.scene_capture.validate_at = Some(Instant::now());
        }
    }

    fn start_scene_capture(&mut self, selection: CaptureSelection) {
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
        self.scene_capture.selection = Some(selection.clone());
        self.scene_capture.retry_at = None;
        self.scene_capture.validate_at = None;
        let Some(root_source) = selection.root.receipt.as_ref() else {
            self.reject_scene_capture(capture_error(
                "project.editor.scene-capture-source-unavailable",
            ));
            return;
        };
        let open_prefabs = selection
            .prefabs
            .iter()
            .filter_map(|selected| {
                let lane = self.prefab_lanes.get(&selected.document)?;
                let source = selected
                    .receipt
                    .as_ref()
                    .or(lane.authoring_source.as_ref())?;
                let session = self.workspace.prefab(selected.document)?.session();
                let snapshot = AuthoringPrefabSessionSnapshot::capture(source, session);
                Some(if selected.receipt.is_some() {
                    snapshot
                } else {
                    snapshot.without_persistence()
                })
            })
            .collect();
        let request = self
            .authoring_project()
            .expect("authoring capture has a project")
            .capture_scene_request(
                root_source,
                self.workspace
                    .scene(selection.root.document)
                    .expect("captured Scene is open")
                    .session(),
                open_prefabs,
            );
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
            Some(task) => self.scene_capture.pending = Some(CaptureWork::Compose(task)),
            None => self
                .reject_scene_capture(capture_error("project.editor.scene-capture-task-rejected")),
        }
    }

    fn reject_scene_capture(&mut self, diagnostics: DiagnosticReport) {
        self.authoring_runtime.invalidate_preview_content();
        self.scene_capture.captured = None;
        self.scene_capture.pending = None;
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
        let requires_recapture = capture.requires_recapture();
        let preview = capture.take_preview();
        let native_ready = preview.is_some();
        let projection = Some(match preview {
            Some(preview) => projection(
                selection.root.document,
                selection.root.revision,
                preview,
                capture.asset_database(),
            ),
            None => EditorPreviewProjection::rejected(capture.diagnostics().clone()),
        });
        let mut participants = Vec::new();
        let mut unsaved = 0;
        let mut sources = Vec::new();
        for prefab in capture.prefabs() {
            let key = prefab.native_source().key();
            let Some(revision) = prefab.session_revision() else {
                sources.push((key.clone(), None));
                continue;
            };
            let Some(source) = selection.prefabs.iter().find(|source| {
                &source.source_key == key && source.revision == revision && source.receipt.is_some()
            }) else {
                self.reject_scene_capture(capture_error(
                    "project.editor.scene-capture-participant-missing",
                ));
                return;
            };
            let document = source.document;
            sources.push((key.clone(), Some(source.clone())));
            participants.push(EditorPreviewParticipant {
                document,
                revision,
                source_key: prefab.native_source().key().clone(),
            });
            unsaved += usize::from(self.workspace.document_is_dirty(document) == Some(true));
        }
        let validation = capture.source_validation();
        if !self.scene_capture.next_generation() {
            return;
        }
        let dependencies =
            EditorPreviewDependencies::captured(self.scene_capture.generation, participants.into());
        let prepared = CapturedScene {
            projection,
            selection,
            dependencies,
            validation,
            unsaved_dependencies: unsaved,
            sources,
            source_capture: Some(capture),
            native_ready,
            requires_recapture,
            verified: true,
        };
        if !self
            .capture_selection()
            .as_ref()
            .is_some_and(|current| prepared.matches(current))
        {
            if self.capture_selection().as_ref() == Some(&prepared.selection) {
                self.reject_scene_capture(capture_error(
                    "project.editor.scene-capture-source-unavailable",
                ));
            } else {
                self.scene_capture.invalidate();
            }
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
        self.scene_capture.retry_at = None;
        self.scene_capture.validate_at = Some(Instant::now() + SOURCE_RECHECK);
    }

    fn poll_scene_capture(&mut self, selection: &CaptureSelection) {
        let Some(work) = self.scene_capture.pending.take() else {
            return;
        };
        match work {
            CaptureWork::Compose(mut task) => {
                if task.return_observer().state() == TaskReturnState::Pending {
                    self.scene_capture.pending = Some(CaptureWork::Compose(task));
                    return;
                }
                if self.scene_capture.pending_stale {
                    self.scene_capture.pending_stale = false;
                    return;
                }
                match task.try_take() {
                    Some(TaskTerminal::Completed(Ok(capture))) => {
                        let original = self
                            .scene_capture
                            .selection
                            .clone()
                            .expect("capture work retains its selection");
                        if original.root != selection.root {
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
                        if original.root == selection.root {
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
                            self.start_scene_capture(selection.clone());
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
                        self.start_scene_capture(selection.clone());
                    }
                }
            }
        }
    }

    pub(super) fn drive_authoring_scene_capture(&mut self) -> Option<EditorPreviewSource> {
        let Some(selection) = self.capture_selection() else {
            self.scene_capture.cancel();
            self.authoring_runtime.invalidate_preview_content();
            return None;
        };
        let current = self.scene_capture.captured.as_ref().map_or_else(
            || self.scene_capture.selection.as_ref() == Some(&selection),
            |capture| capture.matches(&selection),
        );
        if !current {
            self.scene_capture.invalidate();
            self.authoring_runtime.invalidate_preview_content();
            self.poll_scene_capture(&selection);
            self.start_scene_capture(selection.clone());
        } else {
            self.poll_scene_capture(&selection);
        }
        let now = Instant::now();
        if self.scene_capture.pending.is_none() {
            if self.scene_capture.captured.is_none()
                && self
                    .scene_capture
                    .retry_at
                    .is_none_or(|deadline| deadline <= now)
            {
                self.start_scene_capture(selection.clone());
            } else if self
                .scene_capture
                .validate_at
                .is_some_and(|deadline| deadline <= now)
                && let Some(capture) = &self.scene_capture.captured
            {
                if capture.requires_recapture {
                    // Successful receipts cannot observe a previously missing dependency.
                    self.start_scene_capture(selection.clone());
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
            document: selection.root.document,
            revision: selection.root.revision,
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
                && capture.selection.root.document == basis.document
                && capture.selection.root.revision == basis.revision
                && capture.selection.root.schema == basis.schema
        }) else {
            return Ok(None);
        };
        if !self
            .capture_selection()
            .as_ref()
            .is_some_and(|current| capture.matches(current))
        {
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
            if !self
                .capture_selection()
                .as_ref()
                .is_some_and(|current| capture.matches(current))
            {
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

    pub(super) fn release_published_scene_capture(&mut self) {
        if self
            .authoring_runtime
            .current_preview_content_stamp()
            .is_some()
            && let Some(capture) = &mut self.scene_capture.captured
            && capture.native_ready
        {
            capture.source_capture = None;
        }
    }
}

fn projection(
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    preview: CapturedAuthoringPreview,
    database: Arc<nara_asset::ProjectAssetDatabase>,
) -> EditorPreviewProjection {
    EditorPreviewProjection::authoring(
        preview.candidate,
        EditorAuthoringPreviewOmissions::new(document, revision, preview.omissions),
        database,
    )
}

fn dependencies(
    generation: u64,
    sources: &[(nara_scene::SourceDocumentKey, Option<SourceSelection>)],
) -> EditorPreviewDependencies {
    EditorPreviewDependencies::captured(
        generation,
        sources
            .iter()
            .filter_map(|(key, source)| {
                let source = source.as_ref()?;
                Some(EditorPreviewParticipant {
                    document: source.document,
                    revision: source.revision,
                    source_key: key.clone(),
                })
            })
            .collect::<Vec<_>>()
            .into(),
    )
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
                crate::project_content::format_diagnostic::scene_format_diagnostic(
                    "project.editor.scene-capture-source-format-invalid",
                    "A selected Scene dependency has an invalid source format",
                    &source,
                ),
                path.as_str(),
            ));
            report
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
