//! One retained conversion operation, from background acquisition through exact source publication.

use std::{
    fmt::{self, Write},
    mem::size_of,
    sync::Arc,
};

use nara_diagnostic::DiagnosticReport;
use nara_scene::{
    PrefabConversionError, PrefabExpansionOptions, SceneEntityId, SourceDocumentKind,
};
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskReturnState, TaskSpawnRequest, TaskTerminal,
};
use nara_tooling::{
    EditorDocumentKind, EditorDocumentQueryBasis, EditorPrefabConversionAction,
    EditorPrefabConversionActionOutcome as Outcome, EditorPrefabConversionCommitError,
    EditorPrefabConversionId, EditorPrefabConversionMappingRow,
    EditorPrefabConversionPhase as Phase, EditorPrefabConversionRejection as Rejection,
    EditorPrefabConversionResult, EditorPrefabConversionReview, EditorPrefabConversionView,
    PreparedEditorPrefabConversion,
};

use crate::project_host::authoring::{
    AuthoringPrefabSourceCapture, AuthoringProjectSourceAdmissionError, AuthoringSourceValidation,
};

use super::{EditorProjectSession, authoring_sources::AuthoringSourceSelection};

const CONVERSION_DOMAIN: TaskDomainKey = TaskDomainKey::new(0x7072_6662_636f_6e76);
const REVIEW_ROWS: usize = 64;
const REVIEW_BYTES: usize = 128 * 1024;
// Published/current/replacement pages and the current String-to-Arc row copy can coexist.
const REVIEW_RESERVATION: usize =
    4 * REVIEW_BYTES + 3 * REVIEW_ROWS * size_of::<EditorPrefabConversionMappingRow>();

#[derive(Clone)]
struct ConversionSelection {
    root: AuthoringSourceSelection,
    prefabs: Vec<AuthoringSourceSelection>,
}

struct Participant {
    key: nara_scene::SourceDocumentKey,
    project_path: Arc<str>,
    selected: Option<AuthoringSourceSelection>,
}

struct PreparedOperation {
    source: PreparedEditorPrefabConversion,
    capture: AuthoringPrefabSourceCapture,
    root: AuthoringSourceSelection,
    participants: Vec<Participant>,
}

struct PreparedSource {
    source: PreparedEditorPrefabConversion,
    capture: AuthoringPrefabSourceCapture,
}

struct ConversionFailure {
    reason: Rejection,
    diagnostics: DiagnosticReport,
}

enum PendingWork {
    Prepare(TaskHandle<Result<PreparedSource, ConversionFailure>>),
    Validate {
        prepared: PreparedOperation,
        task: TaskHandle<Result<(), ConversionFailure>>,
    },
}

impl PendingWork {
    fn cancel(&self) {
        match self {
            Self::Prepare(task) => {
                task.cancel();
            }
            Self::Validate { task, .. } => {
                task.cancel();
            }
        }
    }

    fn returned(&self) -> bool {
        match self {
            Self::Prepare(task) => task.return_observer().state() != TaskReturnState::Pending,
            Self::Validate { task, .. } => {
                task.return_observer().state() != TaskReturnState::Pending
            }
        }
    }
}

#[derive(Default)]
pub(super) struct EditorPrefabConversionHost {
    sequence: u64,
    view: EditorPrefabConversionView,
    selection: Option<ConversionSelection>,
    prepared: Option<PreparedOperation>,
    pending: Option<PendingWork>,
    cancel_requested: bool,
    #[cfg(test)]
    next_prepare_gate: Option<Box<dyn FnOnce() + Send>>,
    #[cfg(test)]
    next_validation_gate: Option<Box<dyn FnOnce() + Send>>,
}

impl EditorPrefabConversionHost {
    /// Pending ownership survives cancellation until `advance_prefab_conversion` observes return.
    pub(super) fn schema_adoption_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn view(&self) -> EditorPrefabConversionView {
        self.view.clone()
    }

    pub(super) fn cancel(&mut self) {
        if let Some(work) = &self.pending {
            work.cancel();
            self.cancel_requested = true;
            self.view.phase = Phase::Cancelling;
        } else if self.prepared.take().is_some() {
            self.cancelled();
        }
    }

    fn cancelled(&mut self) {
        self.selection = None;
        self.prepared = None;
        self.pending = None;
        self.cancel_requested = false;
        self.view.review = None;
        self.view.phase = Phase::Terminal;
        self.view.result = self
            .view
            .operation
            .map(|operation| EditorPrefabConversionResult::Cancelled { operation });
    }

    fn fail(&mut self, failure: ConversionFailure) {
        self.selection = None;
        self.prepared = None;
        self.pending = None;
        self.cancel_requested = false;
        self.view.review = None;
        self.view.phase = Phase::Terminal;
        self.view.result =
            self.view
                .operation
                .map(|operation| EditorPrefabConversionResult::Failed {
                    operation,
                    reason: failure.reason,
                    diagnostics: failure.diagnostics,
                });
    }
}

impl EditorProjectSession {
    /// Returns immutable operation state; dispatch acknowledgement never means conversion committed.
    pub fn prefab_conversion_view(&self) -> EditorPrefabConversionView {
        self.prefab_conversion.view()
    }

    /// Applies one explicit conversion action without deriving authority from current selection.
    pub fn request_prefab_conversion(&mut self, action: EditorPrefabConversionAction) -> Outcome {
        if let EditorPrefabConversionAction::Prepare { basis, anchor } = action {
            return self.start_prefab_conversion(basis, anchor);
        }
        let operation = match action {
            EditorPrefabConversionAction::Commit { operation }
            | EditorPrefabConversionAction::Cancel { operation }
            | EditorPrefabConversionAction::ReviewPage { operation, .. }
            | EditorPrefabConversionAction::Acknowledge { operation } => operation,
            EditorPrefabConversionAction::Prepare { .. } => unreachable!(),
        };
        if self.prefab_conversion.view.operation != Some(operation) {
            return rejected(Some(operation), Rejection::WrongOperation);
        }
        match action {
            EditorPrefabConversionAction::Cancel { .. } => {
                if matches!(
                    self.prefab_conversion.view.phase,
                    Phase::Idle | Phase::Terminal
                ) {
                    return rejected(Some(operation), Rejection::WrongPhase);
                }
                self.prefab_conversion.cancel();
                accepted(operation, self.prefab_conversion.view.phase)
            }
            EditorPrefabConversionAction::Acknowledge { .. } => {
                if self.prefab_conversion.view.phase != Phase::Terminal
                    || self.prefab_conversion.pending.is_some()
                {
                    return rejected(Some(operation), Rejection::WrongPhase);
                }
                self.prefab_conversion.view = Default::default();
                accepted(operation, Phase::Idle)
            }
            EditorPrefabConversionAction::ReviewPage { offset, .. } => {
                let Some(prepared) = self.prefab_conversion.prepared.as_ref() else {
                    return rejected(Some(operation), Rejection::WrongPhase);
                };
                match review_page(prepared, offset) {
                    Ok(page) => {
                        self.prefab_conversion.view.review = Some(Arc::new(page));
                        accepted(operation, Phase::Review)
                    }
                    Err(reason) => rejected(Some(operation), reason),
                }
            }
            EditorPrefabConversionAction::Commit { .. } => {
                self.validate_prefab_conversion(operation)
            }
            EditorPrefabConversionAction::Prepare { .. } => unreachable!(),
        }
    }

    fn start_prefab_conversion(
        &mut self,
        basis: EditorDocumentQueryBasis,
        anchor: SceneEntityId,
    ) -> Outcome {
        if self.schema_adoption_fenced() {
            return rejected(None, Rejection::Busy);
        }
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return rejected(None, Rejection::Closing);
        }
        if self.prefab_conversion.view.phase != Phase::Idle {
            return rejected(
                None,
                if self.prefab_conversion.view.phase == Phase::Terminal {
                    Rejection::ResultPending
                } else {
                    Rejection::Busy
                },
            );
        }
        if self.authoring_project().is_none() {
            return rejected(None, Rejection::UnsupportedMode);
        }
        if anchor.as_str().len() > REVIEW_BYTES {
            return rejected(None, Rejection::BudgetExceeded);
        }
        let Some(root) = self.authoring_source_selection(basis.document) else {
            return rejected(None, Rejection::SourceUnavailable);
        };
        let Some(receipt) = root.receipt.as_ref() else {
            return rejected(None, Rejection::SourceUnavailable);
        };
        let mut request = match self.workspace.request_complete_prefab_conversion(
            &basis,
            &anchor,
            PrefabExpansionOptions::default(),
        ) {
            Ok(request) => request,
            Err(report) => {
                let _ = self.diagnostics.extend(report.diagnostics);
                return rejected(None, Rejection::SourceRejected);
            }
        };
        let kind = match request.document_kind() {
            EditorDocumentKind::Scene => SourceDocumentKind::Scene,
            EditorDocumentKind::Prefab => SourceDocumentKind::Prefab,
            EditorDocumentKind::Extension(_) => return rejected(None, Rejection::SourceRejected),
        };
        let locator_matches = match kind {
            SourceDocumentKind::Scene => self.workspace.scene(basis.document)
                .and_then(|slot| slot.session().authoring_source()),
            SourceDocumentKind::Prefab => self.workspace.prefab(basis.document)
                .and_then(|slot| slot.session().authoring_source()),
        }.is_some_and(|source| {
            matches!(source.as_asset_ref(), nara_asset::AssetRef::Path(path) if path.as_str() == receipt.relative_path())
        });
        if !locator_matches || root.revision != basis.revision || root.schema != basis.schema {
            return rejected(None, Rejection::StaleSource);
        }
        let prefabs = self.authoring_prefab_selections();
        let snapshots = self.capture_open_prefabs(&prefabs);
        let Some(schema) = basis.schema.as_ref() else {
            return rejected(None, Rejection::SourceRejected);
        };
        let mut context = match self
            .authoring_project()
            .expect("checked authoring mode")
            .capture_prefab_conversion_request(receipt, schema, kind, snapshots)
        {
            Ok(context) => context,
            Err(error) => {
                let failure = admission_failure(error, Rejection::PreparationFailed);
                let _ = self.diagnostics.extend(failure.diagnostics);
                return rejected(None, failure.reason);
            }
        };
        let selection = ConversionSelection { root, prefabs };
        let Some(selection_bytes) = selection_retained_bytes(&selection)
            .and_then(|bytes| bytes.checked_mul(2))
            .and_then(|bytes| bytes.checked_add(anchor.as_str().len()))
            .and_then(|bytes| bytes.checked_add(4 * size_of::<usize>()))
        else {
            return rejected(None, Rejection::BudgetExceeded);
        };
        let Some(sequence) = self.prefab_conversion.sequence.checked_add(1) else {
            return rejected(None, Rejection::IdExhausted);
        };
        let operation = EditorPrefabConversionId::new(self.current_frame().revision(), sequence);
        #[cfg(test)]
        let gate = self.prefab_conversion.next_prepare_gate.take();
        let task = self
            .authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(sequence, CONVERSION_DOMAIN),
                move |_| {
                    let result = (|| {
                        let retained = request
                            .checked_retained_bytes()
                            .and_then(|bytes| bytes.checked_add(selection_bytes))
                            .and_then(|bytes| bytes.checked_add(REVIEW_RESERVATION))
                            .ok_or_else(|| failure(Rejection::BudgetExceeded))?;
                        context.retain_root_input(retained);
                        let mut acquisition = context.acquire().map_err(|error| {
                            admission_failure(error, Rejection::PreparationFailed)
                        })?;
                        request.restrict_working_bytes(acquisition.initial_compiler_limit());
                        let source = request
                            .prepare_with_loader(None, |reference, working| {
                                acquisition.load(reference, working)
                            })
                            .map_err(|error| match error {
                                PrefabConversionError::Source(report) => ConversionFailure {
                                    reason: Rejection::SourceRejected,
                                    diagnostics: report.diagnostics,
                                },
                                PrefabConversionError::Loader(error) => {
                                    admission_failure(error, Rejection::PreparationFailed)
                                }
                            })?;
                        let participant_bytes = acquisition
                            .loaded_prefabs()
                            .try_fold(0usize, |bytes, prefab| {
                                bytes
                                    .checked_add(size_of::<Participant>())?
                                    .checked_add(prefab.native_source().key().retained_bytes())?
                                    .checked_add(prefab.source().project_path().as_str().len())?
                                    .checked_add(4 * size_of::<usize>())
                            })
                            .ok_or_else(|| failure(Rejection::BudgetExceeded))?;
                        let retained = source
                            .checked_retained_bytes()
                            .and_then(|bytes| bytes.checked_add(participant_bytes))
                            .ok_or_else(|| failure(Rejection::BudgetExceeded))?;
                        let capture =
                            acquisition
                                .finish_source_capture(retained)
                                .map_err(|error| {
                                    admission_failure(error, Rejection::PreparationFailed)
                                })?;
                        if capture.requires_recapture() {
                            return Err(failure(Rejection::SourceRejected));
                        }
                        Ok(PreparedSource { source, capture })
                    })();
                    #[cfg(test)]
                    if let Some(gate) = gate {
                        gate();
                    }
                    result
                },
            )
            .into_handle()
            .ok();
        self.prefab_conversion.sequence = sequence;
        self.prefab_conversion.view = EditorPrefabConversionView {
            operation: Some(operation),
            document: Some(basis.document),
            anchor: Some(Arc::from(anchor.as_str())),
            phase: Phase::Preparing,
            review: None,
            result: None,
        };
        self.prefab_conversion.selection = Some(selection);
        match task {
            Some(task) => self.prefab_conversion.pending = Some(PendingWork::Prepare(task)),
            None => self
                .prefab_conversion
                .fail(failure(Rejection::TaskUnavailable)),
        }
        accepted(operation, self.prefab_conversion.view.phase)
    }

    fn conversion_matches(&self, prepared: &PreparedOperation) -> bool {
        if self
            .authoring_source_selection(prepared.root.document)
            .as_ref()
            != Some(&prepared.root)
        {
            return false;
        }
        let current = self.authoring_prefab_selections();
        prepared.participants.iter().all(|participant| {
            let mut matches = current.iter().filter(|source| {
                source.source_key == participant.key
                    || source.project_path == participant.project_path
            });
            matches.next() == participant.selected.as_ref() && matches.next().is_none()
        })
    }

    fn validate_prefab_conversion(&mut self, operation: EditorPrefabConversionId) -> Outcome {
        if self.schema_adoption_fenced() {
            return rejected(Some(operation), Rejection::Busy);
        }
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return rejected(Some(operation), Rejection::Closing);
        }
        if self.prefab_conversion.view.phase != Phase::Review {
            return rejected(Some(operation), Rejection::WrongPhase);
        }
        let Some(prepared) = self.prefab_conversion.prepared.take() else {
            return rejected(Some(operation), Rejection::WrongPhase);
        };
        if !self.conversion_matches(&prepared) {
            self.prefab_conversion.fail(failure(Rejection::StaleSource));
            return accepted(operation, Phase::Terminal);
        }
        let validation = prepared.capture.source_validation();
        let task = spawn_validation(self, validation);
        match task {
            Some(task) => {
                self.prefab_conversion.pending = Some(PendingWork::Validate { prepared, task });
                self.prefab_conversion.view.phase = Phase::Validating;
                self.prefab_conversion.view.review = None;
            }
            None => self
                .prefab_conversion
                .fail(failure(Rejection::TaskUnavailable)),
        }
        accepted(operation, self.prefab_conversion.view.phase)
    }

    pub(super) fn advance_prefab_conversion(&mut self) {
        let Some(work) = self.prefab_conversion.pending.take() else {
            return;
        };
        if !work.returned() {
            self.prefab_conversion.pending = Some(work);
            return;
        }
        if self.prefab_conversion.cancel_requested || self.closing {
            drop(work);
            self.prefab_conversion.cancelled();
            return;
        }
        match work {
            PendingWork::Prepare(mut task) => match task.try_take() {
                Some(TaskTerminal::Completed(Ok(prepared))) => {
                    self.accept_prefab_conversion(prepared)
                }
                Some(TaskTerminal::Completed(Err(error))) => self.prefab_conversion.fail(error),
                _ => self
                    .prefab_conversion
                    .fail(failure(Rejection::PreparationFailed)),
            },
            PendingWork::Validate { prepared, mut task } => match task.try_take() {
                Some(TaskTerminal::Completed(Ok(()))) => self.commit_prefab_conversion(prepared),
                Some(TaskTerminal::Completed(Err(error))) => self.prefab_conversion.fail(error),
                _ => self
                    .prefab_conversion
                    .fail(failure(Rejection::ValidationFailed)),
            },
        }
    }

    fn accept_prefab_conversion(&mut self, prepared: PreparedSource) {
        let Some(selection) = self.prefab_conversion.selection.take() else {
            self.prefab_conversion.fail(failure(Rejection::StaleSource));
            return;
        };
        let mut participants = Vec::new();
        for prefab in prepared.capture.prefabs() {
            let key = prefab.native_source().key();
            let path = prefab.source().project_path().as_str();
            let mut matches = selection
                .prefabs
                .iter()
                .filter(|source| &source.source_key == key || source.project_path.as_ref() == path);
            let selected = matches.next();
            if matches.next().is_some()
                || selected
                    .and_then(|source| source.receipt.as_ref())
                    .is_none()
                    && prefab.session_revision().is_some()
                || selected.map(|source| source.revision) != prefab.session_revision()
            {
                self.prefab_conversion.fail(failure(Rejection::StaleSource));
                return;
            }
            participants.push(Participant {
                key: key.clone(),
                project_path: Arc::from(path),
                selected: selected.cloned(),
            });
        }
        let prepared = PreparedOperation {
            source: prepared.source,
            capture: prepared.capture,
            root: selection.root,
            participants,
        };
        if !self.conversion_matches(&prepared) {
            self.prefab_conversion.fail(failure(Rejection::StaleSource));
            return;
        }
        match review_page(&prepared, 0) {
            Ok(page) => {
                self.prefab_conversion.view.review = Some(Arc::new(page));
                self.prefab_conversion.view.phase = Phase::Review;
                self.prefab_conversion.prepared = Some(prepared);
            }
            Err(reason) => self.prefab_conversion.fail(failure(reason)),
        }
    }

    fn commit_prefab_conversion(&mut self, prepared: PreparedOperation) {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            self.prefab_conversion.fail(failure(Rejection::Closing));
            return;
        }
        if !self.conversion_matches(&prepared) {
            self.prefab_conversion.fail(failure(Rejection::StaleSource));
            return;
        }
        let operation = self
            .prefab_conversion
            .view
            .operation
            .expect("a pending operation has identity");
        match self
            .workspace
            .commit_complete_prefab_conversion_with_loader(prepared.source, |reference| {
                Ok::<_, std::convert::Infallible>(prepared.capture.resolve(reference))
            }) {
            Ok(receipt) => {
                let _ = self
                    .diagnostics
                    .extend(receipt.report().diagnostics.clone());
                self.prefab_conversion.view.phase = Phase::Terminal;
                self.prefab_conversion.view.result =
                    Some(EditorPrefabConversionResult::Committed {
                        operation,
                        receipt: Arc::new(receipt),
                    });
            }
            Err(EditorPrefabConversionCommitError::Document(report)) => {
                self.prefab_conversion.fail(ConversionFailure {
                    reason: Rejection::SourceRejected,
                    diagnostics: report.diagnostics,
                });
            }
            Err(EditorPrefabConversionCommitError::Loader(never)) => match never {},
        }
    }
}

fn spawn_validation(
    session: &mut EditorProjectSession,
    validation: AuthoringSourceValidation,
) -> Option<TaskHandle<Result<(), ConversionFailure>>> {
    #[cfg(test)]
    let gate = session.prefab_conversion.next_validation_gate.take();
    session
        .authoring_runtime
        .persistence_tasks()
        .spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(session.prefab_conversion.sequence, CONVERSION_DOMAIN),
            move |_| {
                #[cfg(test)]
                if let Some(gate) = gate {
                    gate();
                }
                validation
                    .validate()
                    .map_err(|error| admission_failure(error, Rejection::ValidationFailed))
            },
        )
        .into_handle()
        .ok()
}

fn review_page(
    prepared: &PreparedOperation,
    offset: usize,
) -> Result<EditorPrefabConversionReview, Rejection> {
    let entities = prepared.source.entities();
    if offset > entities.len() || offset == entities.len() && offset != 0 {
        return Err(Rejection::InvalidReviewPage);
    }
    let mut remaining = REVIEW_BYTES;
    let mut rows = Vec::new();
    for mapping in entities.iter().skip(offset).take(REVIEW_ROWS) {
        remaining = remaining
            .checked_sub(mapping.entity().as_str().len())
            .ok_or(Rejection::BudgetExceeded)?;
        let mut origin = BoundedText {
            text: String::new(),
            remaining,
        };
        write_review_origin(&mut origin, prepared, mapping.provenance())
            .map_err(|_| Rejection::BudgetExceeded)?;
        remaining = origin.remaining;
        rows.push(EditorPrefabConversionMappingRow {
            origin: Arc::from(origin.text),
            entity: mapping.entity().clone(),
        });
    }
    let next = offset + rows.len();
    Ok(EditorPrefabConversionReview {
        basis: prepared.source.basis().clone(),
        offset,
        total_entities: entities.len(),
        rows,
        next_offset: (next < entities.len()).then_some(next),
        source_count: prepared.capture.prefabs().len(),
        working_copy_count: prepared
            .capture
            .prefabs()
            .iter()
            .filter(|source| source.session_revision().is_some())
            .count(),
    })
}

fn write_review_origin(
    output: &mut impl Write,
    prepared: &PreparedOperation,
    provenance: &nara_scene::SceneEntityProvenance,
) -> fmt::Result {
    let Some(path) = provenance.projection_path() else {
        return write!(output, "{}", provenance.local_entity_id());
    };
    write!(output, "{}", path.root_anchor())?;
    for step in path.steps() {
        match step {
            nara_scene::PrefabProjectionStep::SourceEntity {
                source_document,
                canonical_source,
                entity,
            } => {
                if let Some(source) = prepared
                    .capture
                    .prefabs()
                    .iter()
                    .find(|source| source.native_source().key() == canonical_source)
                {
                    write!(output, " / {}:{entity}", source.source().project_path())?;
                } else {
                    match source_document {
                        nara_asset::AssetRef::Path(path) => {
                            write!(output, " / {path}:{entity}")?;
                        }
                        nara_asset::AssetRef::StableId(id) => {
                            write!(output, " / {id}:{entity}")?;
                        }
                    }
                }
            }
            nara_scene::PrefabProjectionStep::InstanceAddition { entity } => {
                write!(output, " / added:{entity}")?;
            }
        }
    }
    Ok(())
}

struct BoundedText {
    text: String,
    remaining: usize,
}
impl fmt::Write for BoundedText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.remaining = self.remaining.checked_sub(value.len()).ok_or(fmt::Error)?;
        self.text.push_str(value);
        Ok(())
    }
}

fn selection_retained_bytes(selection: &ConversionSelection) -> Option<usize> {
    std::iter::once(&selection.root)
        .chain(&selection.prefabs)
        .try_fold(size_of::<ConversionSelection>(), |bytes, source| {
            bytes
                .checked_add(size_of::<AuthoringSourceSelection>())?
                .checked_add(source.source_key.retained_bytes())?
                .checked_add(source.project_path.len())?
                .checked_add(
                    source
                        .receipt
                        .as_ref()
                        .map_or(Some(0), |receipt| receipt.checked_retained_bytes())?,
                )
        })
}

fn accepted(operation: EditorPrefabConversionId, phase: Phase) -> Outcome {
    Outcome::Accepted { operation, phase }
}
fn rejected(operation: Option<EditorPrefabConversionId>, reason: Rejection) -> Outcome {
    Outcome::Rejected { operation, reason }
}
fn failure(reason: Rejection) -> ConversionFailure {
    ConversionFailure {
        reason,
        diagnostics: super::single_error(
            "project.editor.prefab-conversion-refused",
            "Complete Prefab conversion could not publish its original admitted candidate",
        ),
    }
}
fn admission_failure(
    error: AuthoringProjectSourceAdmissionError,
    stage: Rejection,
) -> ConversionFailure {
    use AuthoringProjectSourceAdmissionError as E;
    match error {
        E::CompilationBudget(diagnostics) => ConversionFailure {
            reason: Rejection::BudgetExceeded,
            diagnostics,
        },
        E::SourcePublication { source, .. } => ConversionFailure {
            reason: stage,
            diagnostics: source.into_diagnostics(),
        },
        E::SourceStructure { source, .. } => ConversionFailure {
            reason: stage,
            diagnostics: source.into_diagnostics(),
        },
        E::NativeSource(error) => {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(error.diagnostic());
            ConversionFailure {
                reason: stage,
                diagnostics,
            }
        }
        E::Budget(_) | E::AccountingOverflow => failure(Rejection::BudgetExceeded),
        E::SourceChanged { path, .. } => source_failure(
            stage,
            "project.editor.prefab-conversion-source-changed",
            "A source changed after preparation; prepare and review a new conversion",
            path.as_str(),
        ),
        E::AmbiguousPrefabSession { path } => source_failure(
            stage,
            "project.editor.prefab-conversion-source-ambiguous",
            "Multiple open Prefab documents claim a required source",
            path.as_str(),
        ),
        E::AssetIdentity { path, .. }
        | E::AssetMetadataChanged { path }
        | E::AssetMetadataFormat { path, .. } => source_failure(
            stage,
            "project.editor.prefab-conversion-lookup-rejected",
            "Required asset metadata or stable source resolution could not be validated",
            path.as_str(),
        ),
        E::SourceFormat { path, .. }
        | E::SourcePath { path, .. }
        | E::SourceHost { path, .. }
        | E::SourceIdentityKindConflict { path }
        | E::SourceTooLarge { path, .. }
        | E::UnsupportedSourceEncoding { path, .. }
        | E::DocumentSourcePath { path, .. } => source_failure(
            stage,
            "project.editor.prefab-conversion-source-rejected",
            "A required source could not be admitted for complete conversion",
            path.as_str(),
        ),
        _ => failure(stage),
    }
}

fn source_failure(
    reason: Rejection,
    code: &'static str,
    summary: &'static str,
    path: &str,
) -> ConversionFailure {
    let mut diagnostic = nara_diagnostic::Diagnostic::error(
        super::super::diagnostic_code(code),
        super::super::safe_summary(summary),
    );
    if let Ok(field) =
        nara_diagnostic::DiagnosticField::project_relative(super::super::field_key("source"), path)
        && let Ok(with_source) = diagnostic.clone().try_with_field(field)
    {
        diagnostic = with_source;
    }
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic);
    ConversionFailure {
        reason,
        diagnostics,
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
