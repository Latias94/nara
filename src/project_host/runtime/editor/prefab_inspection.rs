//! Retained effective Prefab reads and source-fenced edits through the original edit owner.

use std::{mem, sync::Arc};

use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{ComponentPropertyError, ComponentPropertyReadBudget, ComponentValue};
use nara_scene::{
    __private::{
        AuthoringPrefabInspectionError, AuthoringPrefabPropertySetError, ObservedPrefabPropertySet,
        SourceDocumentInput, try_inspect_authoring_prefab_instance_with_loader,
    },
    AuthoringPrefabInstanceSnapshot, AuthoringReadError, PrefabDeclarationError,
    PrefabDeclarationLimits, PrefabDocument, PrefabInstancePath, PrefabOverlaySlot, PrefabRevert,
    SceneCompilationFailureKind, SceneCompilationOptions, SceneDocument, SceneEntityId,
};
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskRejection, TaskReturnState, TaskSpawnRequest,
    TaskTerminal,
};
use nara_tooling::{
    EditorActionOutcome, EditorDocumentKind, EditorDocumentQueryBasis,
    EditorPrefabInspectionAction as Action, EditorPrefabInspectionId,
    EditorPrefabInspectionOutcome as Outcome, EditorPrefabInspectionPhase as Phase,
    EditorPrefabInspectionRejection as Rejection, EditorPrefabInspectionResult as Terminal,
    EditorPrefabInspectionView, EditorPrefabPropertyAddress,
    MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES, MAX_EDITOR_PROPERTY_READ_SCAN_BYTES,
    MAX_EDITOR_PROPERTY_READ_SCAN_ENTRIES, PreparedEditorDocumentEdit,
};

use crate::project_host::authoring::{
    AuthoringPrefabCaptureRequest, AuthoringPrefabSourceCapture,
    AuthoringProjectSourceAdmissionError, AuthoringSceneSessionSnapshot, AuthoringSourceValidation,
};

use super::{
    EditorProjectSession,
    authoring_sources::{
        AuthoringSourceSelection, SelectedPrefabBasis, SelectedPrefabSources,
        authoring_source_failure, prefab_participant_retention,
    },
    prepared_edit::EditorEditPreparationError,
};

const INSPECTION_DOMAIN: TaskDomainKey = TaskDomainKey::new(0x7072_6662_696e_7370);

struct Failure {
    reason: Rejection,
    diagnostics: DiagnosticReport,
}

impl From<Rejection> for Failure {
    fn from(reason: Rejection) -> Self {
        Self {
            reason,
            diagnostics: super::single_error(
                "project.editor.prefab-inspection-refused",
                "The selected Prefab inspection or property edit could not proceed",
            ),
        }
    }
}

struct Inspected {
    snapshot: AuthoringPrefabInstanceSnapshot,
    capture: AuthoringPrefabSourceCapture,
}

struct InspectionBasis {
    document: EditorDocumentQueryBasis,
    selection: SelectedPrefabBasis,
    sources: AuthoringSourceValidation,
}

struct InspectWork {
    document: EditorDocumentQueryBasis,
    selection: SelectedPrefabSources,
    task: TaskHandle<Result<Inspected, Failure>>,
}

struct VerifiedSet {
    edit: PreparedEditorDocumentEdit,
    selection: SelectedPrefabBasis,
}

enum InspectionWork {
    Read(InspectWork),
    Set {
        basis: InspectionBasis,
        task: TaskHandle<Result<VerifiedSet, Failure>>,
    },
}

impl InspectionWork {
    fn cancel(&self) {
        match self {
            Self::Read(work) => work.task.cancel(),
            Self::Set { task, .. } => task.cancel(),
        };
    }

    fn return_state(&self) -> TaskReturnState {
        match self {
            Self::Read(work) => work.task.return_observer(),
            Self::Set { task, .. } => task.return_observer(),
        }
        .state()
    }
}

enum InspectionEdit {
    Revert {
        path: PrefabInstancePath,
        slot: PrefabOverlaySlot,
    },
    Set(ObservedPrefabPropertySet),
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Working(InspectionWork),
    Ready {
        basis: InspectionBasis,
        snapshot: AuthoringPrefabInstanceSnapshot,
    },
    PreparingEdit {
        basis: InspectionBasis,
        change: InspectionEdit,
        limits: PrefabDeclarationLimits,
    },
    Review {
        basis: InspectionBasis,
        edit: PreparedEditorDocumentEdit,
    },
    Validating {
        basis: InspectionBasis,
        edit: PreparedEditorDocumentEdit,
    },
    Cancelling(InspectionWork),
    Terminal(Terminal),
}

impl State {
    const fn phase(&self) -> Phase {
        match self {
            Self::Idle => Phase::Idle,
            Self::Working(InspectionWork::Read(_)) => Phase::Inspecting,
            Self::Working(InspectionWork::Set { .. }) | Self::PreparingEdit { .. } => {
                Phase::PreparingEdit
            }
            Self::Ready { .. } => Phase::Ready,
            Self::Review { .. } => Phase::Review,
            Self::Validating { .. } => Phase::Validating,
            Self::Cancelling(_) => Phase::Cancelling,
            Self::Terminal(_) => Phase::Terminal,
        }
    }
}

#[derive(Default)]
pub(super) struct EditorPrefabInspectionHost {
    sequence: u64,
    inspection: Option<EditorPrefabInspectionId>,
    state: State,
    #[cfg(test)]
    pub(super) next_inspect_gate: Option<Box<dyn FnOnce() + Send>>,
}

impl EditorPrefabInspectionHost {
    pub(super) fn view(&self) -> EditorPrefabInspectionView {
        EditorPrefabInspectionView {
            inspection: self.inspection,
            phase: self.state.phase(),
            result: match &self.state {
                State::Terminal(result) => Some(result.clone()),
                _ => None,
            },
        }
    }

    pub(super) const fn work_pending(&self) -> bool {
        self.state.phase().work_pending()
    }

    pub(super) const fn reclamation_blocked(&self) -> bool {
        !matches!(self.state, State::Idle | State::Terminal(_))
    }

    pub(super) fn cancel(&mut self) {
        self.state = match mem::take(&mut self.state) {
            State::Working(work) => {
                work.cancel();
                State::Cancelling(work)
            }
            State::Ready { .. }
            | State::PreparingEdit { .. }
            | State::Review { .. }
            | State::Validating { .. } => self.cancelled(),
            state => state,
        };
    }

    fn cancelled(&self) -> State {
        State::Terminal(Terminal::Cancelled {
            inspection: self.id(),
        })
    }

    fn id(&self) -> EditorPrefabInspectionId {
        self.inspection
            .expect("an admitted inspection has an identity")
    }
}

impl EditorProjectSession {
    /// Borrows the completed read-only observation while its original working-copy basis matches.
    /// Unobserved disk edits are checked at edit preparation and publication.
    pub fn prefab_effective_observation(
        &self,
        inspection: EditorPrefabInspectionId,
    ) -> Result<&AuthoringPrefabInstanceSnapshot, Rejection> {
        self.check_prefab_inspection_id(inspection)?;
        let State::Ready { basis, snapshot } = &self.prefab_inspection.state else {
            return Err(Rejection::WrongPhase);
        };
        self.check_prefab_inspection_basis(basis)?;
        Ok(snapshot)
    }

    /// Borrows the exact prepared edit. The inspection Host retains its commit authority.
    pub fn prefab_inspection_review(
        &self,
        inspection: EditorPrefabInspectionId,
    ) -> Result<&PreparedEditorDocumentEdit, Rejection> {
        self.check_prefab_inspection_id(inspection)?;
        let State::Review { basis, edit } = &self.prefab_inspection.state else {
            return Err(Rejection::WrongPhase);
        };
        self.check_prefab_inspection_basis(basis)?;
        Ok(edit)
    }

    fn check_prefab_inspection_id(
        &self,
        inspection: EditorPrefabInspectionId,
    ) -> Result<(), Rejection> {
        if self.prefab_inspection.inspection != Some(inspection) {
            return Err(Rejection::WrongInspection);
        }
        Ok(())
    }

    fn check_prefab_inspection_admission(&self) -> Result<(), Rejection> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(Rejection::Closing);
        }
        if self.authoring_operation_fenced() || self.source_tools.reclamation_fenced() {
            return Err(Rejection::Busy);
        }
        Ok(())
    }

    fn check_prefab_inspection_basis(&self, basis: &InspectionBasis) -> Result<(), Rejection> {
        self.check_prefab_inspection_admission()?;
        if !basis.selection.matches(self) {
            return Err(Rejection::StaleSource);
        }
        Ok(())
    }

    pub(super) fn request_prefab_inspection(&mut self, action: Action) -> Outcome {
        let inspection = match action {
            Action::Inspect { basis, anchor } => {
                return self.start_prefab_inspection(basis, anchor);
            }
            Action::PrepareRevert { inspection, .. }
            | Action::PrepareSet { inspection, .. }
            | Action::Commit { inspection }
            | Action::Cancel { inspection }
            | Action::Acknowledge { inspection } => inspection,
        };
        let result = self
            .check_prefab_inspection_id(inspection)
            .and_then(|()| match action {
                Action::PrepareRevert {
                    address, limits, ..
                } => self.start_prefab_inspection_revert(&address, limits),
                Action::PrepareSet {
                    address,
                    value,
                    limits,
                    ..
                } => self.start_prefab_inspection_set(address, value, limits),
                Action::Commit { .. } => self.start_prefab_inspection_commit(),
                Action::Cancel { .. } => {
                    if matches!(
                        self.prefab_inspection.state,
                        State::Terminal(_) | State::Cancelling(_)
                    ) {
                        return Err(Rejection::WrongPhase);
                    }
                    self.prefab_inspection.cancel();
                    Ok(())
                }
                Action::Acknowledge { .. } => {
                    if !matches!(self.prefab_inspection.state, State::Terminal(_)) {
                        return Err(Rejection::WrongPhase);
                    }
                    self.prefab_inspection.state = State::Idle;
                    self.prefab_inspection.inspection = None;
                    Ok(())
                }
                Action::Inspect { .. } => unreachable!(),
            });
        match result {
            Ok(()) => Outcome::Accepted {
                inspection,
                phase: self.prefab_inspection.state.phase(),
            },
            Err(reason) => Outcome::Rejected {
                inspection: Some(inspection),
                reason,
            },
        }
    }

    fn start_prefab_inspection(
        &mut self,
        document: EditorDocumentQueryBasis,
        anchor: SceneEntityId,
    ) -> Outcome {
        match self.admit_prefab_inspection(document, anchor) {
            Ok(inspection) => Outcome::Accepted {
                inspection,
                phase: self.prefab_inspection.state.phase(),
            },
            Err(failure) => {
                let _ = self.diagnostics.extend(failure.diagnostics);
                Outcome::Rejected {
                    inspection: None,
                    reason: failure.reason,
                }
            }
        }
    }

    fn admit_prefab_inspection(
        &mut self,
        document: EditorDocumentQueryBasis,
        anchor: SceneEntityId,
    ) -> Result<EditorPrefabInspectionId, Failure> {
        self.check_prefab_inspection_admission()?;
        match &self.prefab_inspection.state {
            State::Idle => {}
            State::Terminal(_) => return Err(Rejection::ResultPending.into()),
            _ => return Err(Rejection::Busy.into()),
        }
        if self.authoring_project().is_none() {
            return Err(Rejection::UnsupportedMode.into());
        }
        self.workspace
            .validate_document_query_basis(&document)
            .map_err(|_| Rejection::StaleSource)?;
        let schema = document
            .schema
            .clone()
            .ok_or(Rejection::SourceUnavailable)?;
        let sequence = self
            .prefab_inspection
            .sequence
            .checked_add(1)
            .ok_or(Rejection::IdExhausted)?;
        if anchor.as_str().len() > MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES {
            return Err(Rejection::BudgetExceeded.into());
        }
        let root = self
            .authoring_source_selection(document.document)
            .ok_or(Rejection::SourceUnavailable)?;
        let (input, root_bytes) = InspectionDocument::capture(self, &root)?;
        let retained = root_bytes
            .checked_add(anchor.as_str().len())
            .and_then(|bytes| bytes.checked_add(mem::size_of::<InspectionDocument>()))
            .ok_or(Rejection::BudgetExceeded)?;
        let (selection, context) = self.capture_prefab_inspection_sources(root, 0, None)?;
        #[cfg(test)]
        let gate = self.prefab_inspection.next_inspect_gate.take();
        let task = self
            .authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(sequence, INSPECTION_DOMAIN),
                move |_| {
                    let result = inspect_prefab(input.view(), &anchor, &schema, context, retained);
                    #[cfg(test)]
                    if let Some(gate) = gate {
                        gate();
                    }
                    result
                },
            )
            .into_handle()
            .map_err(task_rejection)?;
        let inspection = EditorPrefabInspectionId::new(self.current_frame().revision(), sequence);
        self.prefab_inspection.sequence = sequence;
        self.prefab_inspection.inspection = Some(inspection);
        self.prefab_inspection.state = State::Working(InspectionWork::Read(InspectWork {
            document,
            selection,
            task,
        }));
        Ok(inspection)
    }

    fn capture_prefab_inspection_sources(
        &self,
        root: AuthoringSourceSelection,
        retained: usize,
        prior: Option<&AuthoringSourceValidation>,
    ) -> Result<(SelectedPrefabSources, AuthoringPrefabCaptureRequest), Failure> {
        let project = self.authoring_project().ok_or(Rejection::UnsupportedMode)?;
        let snapshot_bytes = self
            .prefab_source_capture_selection_bytes(root.document)
            .ok_or(Rejection::BudgetExceeded)?;
        let mut inputs = self
            .prefab_working_set_retained_bytes()
            .and_then(|bytes| {
                let bytes = bytes
                    .checked_add(snapshot_bytes)
                    .and_then(|bytes| bytes.checked_add(retained))
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
                project.admit_source_capture_inputs(0, bytes)
            })
            .map_err(|error| source_failure(error, Rejection::PreparationFailed))?;
        if let Some(prior) = prior {
            inputs
                .retain_prior_inspection(prior)
                .map_err(|error| source_failure(error, Rejection::PreparationFailed))?;
        }
        let kind = root.source_key.kind();
        self.capture_selected_prefab_sources(root, kind, inputs)
            .map_err(|error| source_failure(error, Rejection::PreparationFailed))
    }

    fn start_prefab_inspection_revert(
        &mut self,
        address: &EditorPrefabPropertyAddress,
        limits: PrefabDeclarationLimits,
    ) -> Result<(), Rejection> {
        let State::Ready { basis, snapshot } = &self.prefab_inspection.state else {
            return Err(Rejection::WrongPhase);
        };
        self.check_prefab_inspection_basis(basis)?;
        let (path, slot) = exact_containing_claim(snapshot, address)?;
        self.queue_prefab_inspection_edit(InspectionEdit::Revert { path, slot }, limits);
        Ok(())
    }

    fn start_prefab_inspection_set(
        &mut self,
        address: EditorPrefabPropertyAddress,
        value: ComponentValue,
        limits: PrefabDeclarationLimits,
    ) -> Result<(), Rejection> {
        let State::Ready { basis, snapshot } = &self.prefab_inspection.state else {
            return Err(Rejection::WrongPhase);
        };
        self.check_prefab_inspection_basis(basis)?;
        admit_prefab_property_address(&address)?;
        if self.workspace.document_kind(basis.document.document) == Some(EditorDocumentKind::Prefab)
            && super::actions::component_uses_ui_domain(&address.component)
        {
            return Err(Rejection::InvalidProperty);
        }
        let edit = snapshot
            .plan_property_set(
                &address.entity,
                &address.component,
                &address.property,
                value,
                limits,
                &mut prefab_property_read_budget(),
            )
            .map_err(property_set_rejection)?;
        self.queue_prefab_inspection_edit(InspectionEdit::Set(edit), limits);
        Ok(())
    }

    fn queue_prefab_inspection_edit(
        &mut self,
        change: InspectionEdit,
        limits: PrefabDeclarationLimits,
    ) {
        let State::Ready { basis, .. } = mem::take(&mut self.prefab_inspection.state) else {
            unreachable!();
        };
        self.prefab_inspection.state = State::PreparingEdit {
            basis,
            change,
            limits,
        };
    }

    fn start_prefab_inspection_commit(&mut self) -> Result<(), Rejection> {
        let State::Review { basis, .. } = &self.prefab_inspection.state else {
            return Err(Rejection::WrongPhase);
        };
        self.check_prefab_inspection_basis(basis)?;
        let State::Review { basis, edit } = mem::take(&mut self.prefab_inspection.state) else {
            unreachable!();
        };
        self.prefab_inspection.state = State::Validating { basis, edit };
        Ok(())
    }

    pub(super) fn advance_prefab_inspection(&mut self) {
        if !self.prefab_inspection.work_pending() {
            return;
        }
        if self.closing {
            self.prefab_inspection.cancel();
        }
        let state = mem::take(&mut self.prefab_inspection.state);
        let outcome = self.advance_prefab_inspection_state(state);
        self.prefab_inspection.state = match outcome {
            Ok(state) => state,
            Err(failure) => State::Terminal(Terminal::Failed {
                inspection: self.prefab_inspection.id(),
                reason: failure.reason,
                diagnostics: failure.diagnostics,
            }),
        };
    }

    fn advance_prefab_inspection_state(&mut self, state: State) -> Result<State, Failure> {
        match state {
            State::Working(work) => match work.return_state() {
                TaskReturnState::Pending => Ok(State::Working(work)),
                TaskReturnState::CleanupFailed => Err(Rejection::TaskCleanupFailed.into()),
                TaskReturnState::Returned => self.finish_prefab_inspection_work(work),
            },
            State::Cancelling(work) => match work.return_state() {
                TaskReturnState::Pending => Ok(State::Cancelling(work)),
                TaskReturnState::CleanupFailed => Err(Rejection::TaskCleanupFailed.into()),
                TaskReturnState::Returned => Ok(self.prefab_inspection.cancelled()),
            },
            State::PreparingEdit {
                basis,
                change,
                limits,
            } => {
                self.revalidate_prefab_inspection_sources(&basis)?;
                match change {
                    InspectionEdit::Revert { path, slot } => {
                        let edit = self
                            .prepare_prefab_revert(
                                &basis.document,
                                &path,
                                PrefabRevert::Intent(slot),
                                limits,
                            )
                            .map_err(preparation_failure)?;
                        Ok(State::Review { basis, edit })
                    }
                    InspectionEdit::Set(edit) => self
                        .start_prefab_set_verification(basis, edit, limits)
                        .map(State::Working),
                }
            }
            State::Validating { basis, edit } => {
                self.revalidate_prefab_inspection_sources(&basis)?;
                let document = edit.document();
                let reverted = edit.is_prefab_revert();
                let outcome = if reverted {
                    self.commit_authoring_edit(edit)
                } else {
                    self.commit_verified_prefab_set(edit)
                };
                match outcome {
                    EditorActionOutcome::Workspace(report) if report.applied => {
                        let inspection = self.prefab_inspection.id();
                        let revision = report.revision.expect("applied source edit has a revision");
                        Ok(State::Terminal(if reverted {
                            Terminal::Reverted {
                                inspection,
                                document,
                                revision,
                            }
                        } else {
                            Terminal::Set {
                                inspection,
                                document,
                                revision,
                            }
                        }))
                    }
                    EditorActionOutcome::Workspace(report) => Err(Failure {
                        reason: Rejection::SourceRejected,
                        diagnostics: report.diagnostics,
                    }),
                    _ => Err(Rejection::SourceRejected.into()),
                }
            }
            state => Ok(state),
        }
    }

    fn finish_prefab_inspection_work(&self, work: InspectionWork) -> Result<State, Failure> {
        match work {
            InspectionWork::Read(mut work) => {
                let Some(TaskTerminal::Completed(result)) = work.task.try_take() else {
                    return Err(Rejection::PreparationFailed.into());
                };
                let Inspected { snapshot, capture } = result?;
                let selection = work
                    .selection
                    .complete(&capture)
                    .ok_or(Rejection::StaleSource)?;
                let retained = selection
                    .checked_retained_bytes()
                    .ok_or(Rejection::BudgetExceeded)?;
                let sources = capture
                    .inspection_source_validation(retained)
                    .map_err(|error| source_failure(error, Rejection::ValidationFailed))?;
                let basis = InspectionBasis {
                    document: work.document,
                    selection,
                    sources,
                };
                self.check_prefab_inspection_basis(&basis)?;
                Ok(State::Ready { basis, snapshot })
            }
            InspectionWork::Set { basis, mut task } => {
                self.check_prefab_inspection_basis(&basis)?;
                let Some(TaskTerminal::Completed(result)) = task.try_take() else {
                    return Err(Rejection::PreparationFailed.into());
                };
                let VerifiedSet { edit, selection } = result?;
                if selection != basis.selection {
                    return Err(Rejection::StaleSource.into());
                }
                Ok(State::Review { basis, edit })
            }
        }
    }

    fn start_prefab_set_verification(
        &mut self,
        basis: InspectionBasis,
        set: ObservedPrefabPropertySet,
        limits: PrefabDeclarationLimits,
    ) -> Result<InspectionWork, Failure> {
        let edit = self
            .workspace
            .prepare_prefab_property_set(&basis.document, &set, limits)
            .map_err(|report| {
                preparation_failure(EditorEditPreparationError::Document(Box::new(report)))
            })?;
        let root = self
            .authoring_source_selection(basis.document.document)
            .ok_or(Rejection::StaleSource)?;
        let retained = edit
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(set.checked_retained_bytes()?))
            .ok_or(Rejection::BudgetExceeded)?;
        let (selection, mut context) =
            self.capture_prefab_inspection_sources(root, retained, Some(&basis.sources))?;
        if let Some(document) = edit.shared_prefab_document() {
            context
                .use_containing_candidate(document)
                .map_err(|error| source_failure(error, Rejection::PreparationFailed))?;
        }
        let schema = basis
            .document
            .schema
            .clone()
            .ok_or(Rejection::StaleSource)?;
        let sources = basis.sources.clone();
        #[cfg(test)]
        let gate = self.prefab_inspection.next_inspect_gate.take();
        let task = self
            .authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(self.prefab_inspection.sequence, INSPECTION_DOMAIN),
                move |_| {
                    let Inspected { snapshot, capture } =
                        inspect_prefab(edit.source_input(), set.anchor(), &schema, context, 0)?;
                    if !sources.has_same_source_inputs(&capture.source_validation()) {
                        return Err(Rejection::StaleSource.into());
                    }
                    let selection = selection.complete(&capture).ok_or(Rejection::StaleSource)?;
                    if !snapshot
                        .verifies_property_set(&set, &mut prefab_property_read_budget())
                        .map_err(property_set_rejection)?
                    {
                        return Err(Rejection::InvalidProperty.into());
                    }
                    #[cfg(test)]
                    if let Some(gate) = gate {
                        gate();
                    }
                    Ok(VerifiedSet { edit, selection })
                },
            )
            .into_handle()
            .map_err(task_rejection)?;
        Ok(InspectionWork::Set { basis, task })
    }

    fn revalidate_prefab_inspection_sources(&self, basis: &InspectionBasis) -> Result<(), Failure> {
        self.check_prefab_inspection_basis(basis)?;
        basis
            .sources
            .clone()
            .validate()
            .map_err(|error| source_failure(error, Rejection::ValidationFailed))
    }
}

enum InspectionDocument {
    Scene(Arc<SceneDocument>),
    Prefab(Arc<PrefabDocument>),
}

impl InspectionDocument {
    fn capture(
        editor: &EditorProjectSession,
        source: &AuthoringSourceSelection,
    ) -> Result<(Self, usize), Failure> {
        let receipt = source
            .receipt
            .as_ref()
            .ok_or(Rejection::SourceUnavailable)?;
        let (locator, capture, bytes) = if let Some(slot) = editor.workspace.scene(source.document)
        {
            let session = slot.session();
            (
                session.authoring_source(),
                Self::Scene(session.shared_document()),
                AuthoringSceneSessionSnapshot::capture_retained_bytes(receipt, session),
            )
        } else {
            let session = editor
                .workspace
                .prefab(source.document)
                .ok_or(Rejection::SourceUnavailable)?
                .session();
            (
                session.authoring_source(),
                Self::Prefab(session.shared_document()),
                Ok(0),
            )
        };
        if !locator.is_some_and(|source| {
            matches!(source.as_asset_ref(), nara_asset::AssetRef::Path(path) if path.as_str() == receipt.relative_path())
        }) {
            return Err(Rejection::StaleSource.into());
        }
        let bytes = bytes.map_err(|error| source_failure(error, Rejection::PreparationFailed))?;
        Ok((capture, bytes))
    }

    fn view(&self) -> SourceDocumentInput<'_> {
        match self {
            Self::Scene(document) => SourceDocumentInput::Scene(document),
            Self::Prefab(document) => SourceDocumentInput::Prefab(document),
        }
    }
}

fn inspect_prefab(
    input: SourceDocumentInput<'_>,
    anchor: &SceneEntityId,
    schema: &nara_scene::AuthoringSchemaBasis,
    mut capture: AuthoringPrefabCaptureRequest,
    retained: usize,
) -> Result<Inspected, Failure> {
    capture.retain_root_input(retained);
    let mut acquisition = capture
        .acquire()
        .map_err(|error| source_failure(error, Rejection::PreparationFailed))?;
    let mut options = SceneCompilationOptions::default();
    let limit = acquisition.initial_compiler_limit();
    if limit.get() < options.limits.working_bytes().get() {
        options.limits = options.limits.with_working_bytes(limit);
    }
    let snapshot = try_inspect_authoring_prefab_instance_with_loader(
        input,
        anchor,
        schema,
        options,
        |reference, working| acquisition.load(reference, working),
    )
    .map_err(|error| match error {
        AuthoringPrefabInspectionError::Loader(error) => {
            source_failure(error, Rejection::SourceRejected)
        }
        AuthoringPrefabInspectionError::Compilation {
            failure,
            diagnostics,
        } => Failure {
            reason: match failure.reason() {
                SceneCompilationFailureKind::WorkLimit => Rejection::BudgetExceeded,
                SceneCompilationFailureKind::SourceChanged => Rejection::StaleSource,
                _ => Rejection::SourceRejected,
            },
            diagnostics,
        },
    })?;
    let retained = acquisition
        .loaded_prefabs()
        .try_fold(
            snapshot
                .checked_retained_bytes()
                .ok_or(Rejection::BudgetExceeded)?,
            |bytes, prefab| {
                bytes.checked_add(prefab_participant_retention(
                    prefab.native_source().key(),
                    prefab.source().project_path().as_str(),
                )?)
            },
        )
        .ok_or(Rejection::BudgetExceeded)?;
    let capture = acquisition
        .finish_source_capture(retained)
        .map_err(|error| source_failure(error, Rejection::ValidationFailed))?;
    if capture.requires_recapture() {
        return Err(Rejection::SourceRejected.into());
    }
    Ok(Inspected { snapshot, capture })
}

fn exact_containing_claim(
    snapshot: &AuthoringPrefabInstanceSnapshot,
    address: &EditorPrefabPropertyAddress,
) -> Result<(PrefabInstancePath, PrefabOverlaySlot), Rejection> {
    admit_prefab_property_address(address)?;
    let component = snapshot
        .target(&address.entity)
        .ok_or(Rejection::InvalidProperty)?
        .component(&address.component)
        .map_err(|_| Rejection::InvalidProperty)?;
    let property = component
        .property_value(&address.property, &mut prefab_property_read_budget())
        .map_err(property_read_rejection)?;
    let claim = property.revert_claim().ok_or(Rejection::NoCurrentClaim)?;
    Ok((claim.location.path().clone(), claim.slot))
}

fn admit_prefab_property_address(address: &EditorPrefabPropertyAddress) -> Result<(), Rejection> {
    address
        .entity
        .logical_bytes_up_to(MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES)
        .ok_or(Rejection::BudgetExceeded)?;
    Ok(())
}

fn prefab_property_read_budget() -> ComponentPropertyReadBudget {
    ComponentPropertyReadBudget::new(
        ItemLimit::new(MAX_EDITOR_PROPERTY_READ_SCAN_ENTRIES).expect("positive scan cap"),
        ByteLimit::new(MAX_EDITOR_PROPERTY_READ_SCAN_BYTES).expect("positive scan cap"),
    )
}

fn property_read_rejection(error: AuthoringReadError) -> Rejection {
    match error {
        AuthoringReadError::Property(
            ComponentPropertyError::ReadBudgetExceeded
            | ComponentPropertyError::PathDepthLimit
            | ComponentPropertyError::PathByteLimit,
        ) => Rejection::BudgetExceeded,
        _ => Rejection::InvalidProperty,
    }
}

fn property_set_rejection(error: AuthoringPrefabPropertySetError) -> Rejection {
    match error {
        AuthoringPrefabPropertySetError::Read(error) => property_read_rejection(error),
        AuthoringPrefabPropertySetError::InvalidProperty => Rejection::InvalidProperty,
        AuthoringPrefabPropertySetError::BudgetExceeded => Rejection::BudgetExceeded,
    }
}

fn task_rejection(error: TaskRejection) -> Rejection {
    match error.return_state {
        TaskReturnState::CleanupFailed => Rejection::TaskCleanupFailed,
        _ => Rejection::TaskUnavailable,
    }
}

fn source_failure(error: AuthoringProjectSourceAdmissionError, stage: Rejection) -> Failure {
    use AuthoringProjectSourceAdmissionError as E;
    let reason = match error {
        E::Budget(_) | E::AccountingOverflow | E::CompilationBudget(_) => Rejection::BudgetExceeded,
        E::SourceChanged { .. }
        | E::AssetMetadataChanged { .. }
        | E::DocumentSourceMismatch
        | E::SchemaBasisMismatch => Rejection::StaleSource,
        _ => stage,
    };
    let (_, diagnostics) = authoring_source_failure(error);
    Failure {
        reason,
        diagnostics,
    }
}

fn preparation_failure(error: EditorEditPreparationError) -> Failure {
    match error {
        EditorEditPreparationError::Document(report) => {
            let reason = report
                .diagnostics
                .iter()
                .find_map(|diagnostic| {
                    let code = diagnostic.code().as_str();
                    if code == PrefabDeclarationError::BudgetExceeded.diagnostic_code() {
                        Some(Rejection::BudgetExceeded)
                    } else if code == PrefabDeclarationError::ConflictingClaim.diagnostic_code() {
                        Some(Rejection::ConflictingClaim)
                    } else if code == PrefabDeclarationError::StaleObservedIntent.diagnostic_code()
                    {
                        Some(Rejection::StaleSource)
                    } else if code == PrefabDeclarationError::AlreadySet.diagnostic_code() {
                        Some(Rejection::AlreadySet)
                    } else {
                        None
                    }
                })
                .unwrap_or(Rejection::SourceRejected);
            Failure {
                reason,
                diagnostics: report.diagnostics,
            }
        }
        EditorEditPreparationError::QueryBasis(_) => Rejection::StaleSource.into(),
        EditorEditPreparationError::Closing => Rejection::Closing.into(),
        EditorEditPreparationError::Busy => Rejection::Busy.into(),
        _ => Rejection::PreparationFailed.into(),
    }
}
