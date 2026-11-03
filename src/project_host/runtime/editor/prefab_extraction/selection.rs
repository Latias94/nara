//! Save selection observes the captured open set and revises its retained preparation.

use super::*;

/// The original captured open sources, not a new Workspace or filesystem query.
pub struct EditorPrefabExtractionSelection<'a> {
    sources: &'a [AuthoringSourceSelection],
}

impl EditorPrefabExtractionSelection<'_> {
    pub fn documents(
        &self,
    ) -> impl ExactSizeIterator<Item = EditorPrefabExtractionSelectionDocument<'_>> + '_ {
        self.sources
            .iter()
            .map(|source| EditorPrefabExtractionSelectionDocument { source })
    }
}

pub struct EditorPrefabExtractionSelectionDocument<'a> {
    source: &'a AuthoringSourceSelection,
}

impl EditorPrefabExtractionSelectionDocument<'_> {
    pub fn document(&self) -> EditorDocumentId {
        self.source.document
    }
    pub fn source_key(&self) -> &nara_scene::SourceDocumentKey {
        &self.source.source_key
    }
    pub fn project_path(&self) -> &str {
        self.source
            .receipt
            .as_ref()
            .expect("complete extraction inputs are physically bound")
            .relative_path()
    }
}

impl EditorProjectSession {
    pub fn prefab_extraction_selection(
        &self,
        operation: EditorPrefabExtractionId,
    ) -> Result<EditorPrefabExtractionSelection<'_>, EditorPrefabExtractionError> {
        if self.prefab_extraction.view.operation != Some(operation)
            || !self.prefab_extraction.view.can_revise()
        {
            return Err(EditorPrefabExtractionError::Operation);
        }
        Ok(EditorPrefabExtractionSelection {
            sources: &self.prefab_extraction.sources,
        })
    }

    /// Corrects or revises selection against the original captured source/history owners.
    /// Actual preparation runs on existing finite Tasks; no source is installed or recaptured.
    pub fn revise_prefab_extraction(
        &mut self,
        operation: EditorPrefabExtractionId,
        save_current: Vec<EditorDocumentId>,
    ) -> Result<(), EditorPrefabExtractionError> {
        use EditorPrefabExtractionError as E;
        if !self.accepts_new_work() {
            return Err(E::Closing);
        }
        if self.prefab_extraction.view.operation != Some(operation)
            || !self.prefab_extraction.view.can_revise()
        {
            return Err(E::Operation);
        }
        if self.workspace.len() != self.prefab_extraction.sources.len()
            || !self
                .prefab_extraction
                .sources
                .iter()
                .all(|source| self.authoring_source_selection_is_current(source))
        {
            return Err(E::Stale);
        }
        let project = self.authoring_project().ok_or(E::NotAuthoring)?;
        if self
            .prefab_extraction
            .review
            .as_ref()
            .is_some_and(|review| !review.matches(project))
            || self
                .prefab_extraction
                .preparation_failure
                .as_ref()
                .is_some_and(|failure| !failure.matches(project))
        {
            return Err(E::Stale);
        }
        if let Some(input) = self.prefab_extraction.unstarted.take() {
            let PreparationInput::Revision(request) = input else {
                unreachable!("initial Task refusal ends before an editable captured preparation")
            };
            self.prefab_extraction.preparation_failure = Some(request.into_failure(None));
        }
        let revised = if let Some(review) = self.prefab_extraction.review.take() {
            review.into_revision(save_current)
        } else {
            self.prefab_extraction
                .preparation_failure
                .take()
                .expect("editable selection retains its exact preparation")
                .into_revision(save_current)
        };
        match revised {
            Ok(request) => {
                self.start_prefab_extraction_preparation(PreparationInput::Revision(request))
            }
            Err(failure) => {
                let reason = preparation_reason(failure.error());
                self.observe_prefab_extraction_preparation_failure(failure);
                Err(reason)
            }
        }
    }

    pub(super) fn start_prefab_extraction_preparation(
        &mut self,
        input: PreparationInput,
    ) -> Result<(), EditorPrefabExtractionError> {
        let operation = self
            .prefab_extraction
            .view
            .operation
            .expect("accepted operation retains identity");
        #[cfg(test)]
        let gate = self.prefab_extraction.next_preparation_gate.take();
        match PreparationWork::start(
            self.authoring_runtime.persistence_tasks(),
            operation,
            input,
            #[cfg(test)]
            gate,
        ) {
            Ok(pending) => {
                self.prefab_extraction.pending = Some(pending);
                self.prefab_extraction.view.phase = EditorPrefabExtractionPhase::Preparing;
                self.prefab_extraction.view.work_pending = true;
                self.prefab_extraction.view.retry_allowed = false;
                self.prefab_extraction.view.refusal = None;
                self.prefab_extraction.diagnostics = None;
                Ok(())
            }
            Err(failure) => {
                let reason = failure
                    .error
                    .as_ref()
                    .map_or(EditorPrefabExtractionError::Task, preparation_reason);
                self.observe_prefab_extraction_unstarted(failure);
                Err(reason)
            }
        }
    }

    pub(super) fn observe_prefab_extraction_unstarted(&mut self, failure: PreparationStartFailure) {
        let PreparationStartFailure { input, error } = failure;
        let (reason, diagnostics) = error.map_or(
            (EditorPrefabExtractionError::Task, None),
            preparation_refusal,
        );
        if matches!(input, PreparationInput::Initial(_)) {
            drop(input);
            self.fail_prefab_extraction(reason, diagnostics);
            return;
        }
        self.prefab_extraction.unstarted = Some(input);
        self.prefab_extraction.view.phase = EditorPrefabExtractionPhase::AwaitingSelection;
        self.prefab_extraction.view.work_pending = false;
        self.prefab_extraction.view.refusal = Some(reason);
        self.prefab_extraction.view.retry_allowed = reason == EditorPrefabExtractionError::Task;
        let (code, summary) = match reason {
            EditorPrefabExtractionError::WorkLimit => (
                "project.editor.prefab-extraction-work-limit",
                "The original Host work allowance refused selection preparation",
            ),
            EditorPrefabExtractionError::SourceLimit => (
                "project.editor.prefab-extraction-source-limit",
                "The original Source allowance refused selection preparation",
            ),
            EditorPrefabExtractionError::OpenHandleLimit => (
                "project.editor.prefab-extraction-handle-limit",
                "The original native-handle ceiling refused selection preparation",
            ),
            _ => (
                "project.editor.prefab-extraction-task-unavailable",
                "The selection preparation did not start; its original input remains available",
            ),
        };
        self.prefab_extraction.diagnostics = diagnostics.or_else(|| {
            Some(crate::project_host::runtime_diagnostics::single_error(
                code, summary,
            ))
        });
    }

    pub(super) fn observe_prefab_extraction_preparation_failure(
        &mut self,
        failure: PrefabExtractionSessionPreparationFailure,
    ) {
        if !failure.has_retained_preparation() {
            let (reason, diagnostics) = preparation_refusal(
                failure
                    .into_error()
                    .expect("a terminal refusal has no editable preparation to discard"),
            );
            self.fail_prefab_extraction(reason, diagnostics);
            return;
        }
        if !self
            .authoring_project()
            .is_some_and(|project| failure.matches(project))
            || self.workspace.len() != self.prefab_extraction.sources.len()
            || !self
                .prefab_extraction
                .sources
                .iter()
                .all(|source| self.authoring_source_selection_is_current(source))
        {
            drop(failure);
            self.fail_prefab_extraction(EditorPrefabExtractionError::Stale, None);
            return;
        }
        let reason = preparation_reason(failure.error());
        self.prefab_extraction.view.sources = failure.source_count();
        self.prefab_extraction.view.documents = failure.open_count();
        self.prefab_extraction.preparation_failure = Some(failure);
        self.prefab_extraction.view.phase = EditorPrefabExtractionPhase::AwaitingSelection;
        self.prefab_extraction.view.work_pending = false;
        self.prefab_extraction.view.refusal = Some(reason);
        self.prefab_extraction.view.retry_allowed = false;
        self.prefab_extraction.diagnostics = None;
    }
}
