//! Selected override promotion, with the destination's ordinary Save preceding removal.

use nara_scene::__private::{AuthoringPrefabPropertyApplyError, ObservedPrefabPropertyApply};

use super::*;

pub(super) struct ApplyRequest {
    declaration: InspectionRevert,
    property: ObservedPrefabPropertyApply,
    destination: EditorDocumentQueryBasis,
    original_destination: AuthoringSourceSelection,
}

pub(super) struct PreparedApply {
    containing: PreparedEditorDocumentEdit,
    destination_edit: Option<PreparedEditorDocumentEdit>,
    destination: EditorDocumentQueryBasis,
    original_destination: AuthoringSourceSelection,
}

impl PreparedApply {
    pub(super) fn review(&self) -> EditorPrefabApplyReview<'_> {
        EditorPrefabApplyReview {
            containing: &self.containing,
            destination: &self.destination,
            destination_edit: self.destination_edit.as_ref(),
        }
    }

    pub(super) fn require_confirmed_inputs(
        &self,
        editor: &EditorProjectSession,
        basis: &InspectionBasis,
    ) -> Result<(), Rejection> {
        require_confirmed_apply_inputs(editor, basis, &self.original_destination)
    }
}

pub(super) struct StartedApply {
    basis: InspectionBasis,
    containing: PreparedEditorDocumentEdit,
    original_destination: AuthoringSourceSelection,
    destination: EditorPrefabApplyDestination,
    requires_exact_save: bool,
}

impl StartedApply {
    pub(super) const fn destination(&self) -> EditorPrefabApplyDestination {
        self.destination
    }

    fn matches_retained_inputs(&self, editor: &EditorProjectSession) -> bool {
        self.basis
            .selection
            .matches_after_prefab_apply(editor, &self.original_destination)
            && self.basis.additional.as_ref().is_none_or(|additional| {
                additional
                    .selection
                    .matches_after_prefab_apply(editor, &self.original_destination)
            })
    }
}

impl EditorProjectSession {
    pub(super) fn start_prefab_inspection_apply(
        &mut self,
        address: &EditorPrefabPropertyAddress,
        destination: EditorDocumentQueryBasis,
        limits: PrefabDeclarationLimits,
    ) -> Result<(), Rejection> {
        let State::Ready { basis, snapshot } = &self.prefab_inspection.state else {
            return Err(Rejection::WrongPhase);
        };
        self.check_prefab_inspection_basis(basis)?;
        if destination.document == basis.document.document
            || self.workspace.document_kind(destination.document)
                != Some(EditorDocumentKind::Prefab)
        {
            return Err(Rejection::InvalidDestination);
        }
        if super::super::actions::component_uses_ui_domain(&address.component) {
            return Err(Rejection::InvalidProperty);
        }
        self.workspace
            .validate_document_query_basis(&destination)
            .map_err(|_| Rejection::StaleSource)?;
        let original_destination = self
            .authoring_source_selection(destination.document)
            .filter(|source| source.receipt.is_some() && basis.selection.used_working_copy(source))
            .ok_or(Rejection::InvalidDestination)?;
        require_confirmed_apply_inputs(self, basis, &original_destination)?;
        let declaration = exact_containing_claim(snapshot, address)?;
        let claim = self
            .current_inspected_property_claim(basis.document.document, &declaration)
            .map_err(|failure| failure.reason)?;
        let property = snapshot
            .plan_property_apply(
                &address.entity,
                &address.component,
                &address.property,
                claim,
                &original_destination.source_key,
                limits,
                &mut prefab_property_read_budget(),
            )
            .map_err(apply_rejection)?;
        self.queue_prefab_inspection_edit(
            InspectionEdit::Apply(Box::new(ApplyRequest {
                declaration,
                property,
                destination,
                original_destination,
            })),
            limits,
        );
        Ok(())
    }

    pub(super) fn start_prefab_apply_verification(
        &mut self,
        basis: InspectionBasis,
        request: ApplyRequest,
        limits: PrefabDeclarationLimits,
    ) -> Result<InspectionWork, Failure> {
        let ApplyRequest {
            declaration,
            property,
            destination,
            original_destination,
        } = request;
        require_confirmed_apply_inputs(self, &basis, &original_destination)?;
        let claim = self.current_inspected_property_claim(basis.document.document, &declaration)?;
        let containing = self
            .prepare_prefab_revert(
                &basis.document,
                declaration.claim.location.path(),
                PrefabRevert::Property(claim),
                limits,
            )
            .map_err(preparation_failure)?;
        let destination_edit = self
            .workspace
            .prepare_prefab_property_apply(&destination, &property)
            .map_err(|report| {
                preparation_failure(EditorEditPreparationError::Document(Box::new(report)))
            })?;
        if let Some(edit) = &destination_edit {
            self.verify_prepared_patch_contract(edit.document(), edit.patch())
                .map_err(|_| Rejection::InvalidProperty)?;
        }
        let retained = containing
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(property.checked_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(original_destination.checked_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(size_of::<PreparedApply>()))
            .and_then(|bytes| {
                bytes.checked_add(match &destination_edit {
                    Some(edit) => edit.checked_retained_bytes()?,
                    None => 0,
                })
            })
            .ok_or(Rejection::BudgetExceeded)?;
        let root = self
            .authoring_source_selection(basis.document.document)
            .ok_or(Rejection::StaleSource)?;
        let (selection, mut context) =
            self.capture_prefab_inspection_sources(root, retained, Some(&basis.sources))?;
        let map_source = |error| source_failure(error, Rejection::PreparationFailed);
        if let Some(document) = containing.shared_prefab_document() {
            context
                .use_containing_candidate(document)
                .map_err(map_source)?;
        }
        if let Some(document) = destination_edit
            .as_ref()
            .and_then(PreparedEditorDocumentEdit::shared_prefab_document)
        {
            context
                .use_prefab_destination_candidate(
                    original_destination
                        .receipt
                        .as_ref()
                        .ok_or(Rejection::StaleSource)?,
                    document,
                )
                .map_err(map_source)?;
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
                    let inspected = inspect_prefab(
                        PrefabInspectionInput::Borrowed(containing.source_input()),
                        property.anchor(),
                        &schema,
                        context,
                        0,
                    )?;
                    if !inspected
                        .snapshot
                        .verifies_property_apply(&property, &mut prefab_property_read_budget())
                        .map_err(apply_rejection)?
                    {
                        return Err(Rejection::ConflictingClaim.into());
                    }
                    let edit = InspectionReview::Apply(Box::new(PreparedApply {
                        containing,
                        destination_edit,
                        destination,
                        original_destination,
                    }));
                    let verified = verified_edit(edit, selection, &sources, &inspected.capture)?;
                    #[cfg(test)]
                    if let Some(gate) = gate {
                        gate();
                    }
                    Ok(verified)
                },
            )
            .into_handle()
            .map_err(task_rejection)?;
        Ok(InspectionWork::Verify { basis, task })
    }

    pub(super) fn publish_prefab_apply_destination(
        &mut self,
        basis: InspectionBasis,
        apply: PreparedApply,
    ) -> Result<State, Failure> {
        apply.require_confirmed_inputs(self, &basis)?;
        let PreparedApply {
            containing,
            destination_edit,
            destination,
            original_destination,
        } = apply;
        self.workspace
            .validate_document_query_basis(&destination)
            .map_err(|_| Rejection::StaleSource)?;
        let changed = destination_edit.is_some();
        let requires_exact_save =
            changed || self.workspace.document_is_dirty(destination.document) == Some(true);
        let revision = if let Some(edit) = destination_edit {
            applied_revision(self.commit_authoring_edit(edit))?
        } else {
            destination.revision
        };
        let destination = EditorPrefabApplyDestination {
            document: destination.document,
            revision,
            changed,
        };
        Ok(State::AwaitingDestinationSave(Box::new(StartedApply {
            basis,
            containing,
            original_destination,
            destination,
            requires_exact_save,
        })))
    }

    pub(super) fn start_finish_prefab_apply(&mut self) -> Result<(), Rejection> {
        let State::AwaitingDestinationSave(apply) = &self.prefab_inspection.state else {
            return Err(Rejection::WrongPhase);
        };
        self.check_prefab_inspection_admission()?;
        let destination = apply.destination();
        if self
            .workspace
            .prefab(destination.document)
            .is_some_and(|slot| {
                slot.revision() == destination.revision
                    && (slot.is_dirty()
                        || self.persistence.has_pending_document(destination.document))
            })
            && apply.matches_retained_inputs(self)
        {
            return Err(Rejection::DestinationSaveRequired);
        }
        let State::AwaitingDestinationSave(apply) = mem::take(&mut self.prefab_inspection.state)
        else {
            unreachable!("the checked Apply stage cannot change during dispatch");
        };
        self.prefab_inspection.state = State::FinishingApply(apply);
        Ok(())
    }

    fn saved_apply_destination(
        &self,
        apply: &StartedApply,
    ) -> Result<AuthoringSourceSelection, Rejection> {
        self.check_prefab_inspection_admission()?;
        let destination = apply.destination;
        let slot = self
            .workspace
            .prefab(destination.document)
            .ok_or(Rejection::StaleSource)?;
        if slot.revision() != destination.revision {
            return Err(Rejection::StaleSource);
        }
        if slot.is_dirty() || self.persistence.has_pending_document(destination.document) {
            return Err(Rejection::DestinationSaveRequired);
        }
        let saved = self
            .authoring_source_selection(destination.document)
            .ok_or(Rejection::StaleSource)?;
        if saved.source_key != apply.original_destination.source_key
            || saved.project_path != apply.original_destination.project_path
            || saved.schema != apply.original_destination.schema
            || !saved.has_confirmed_checkpoint(self)
            || (apply.requires_exact_save && slot.saved_revision() != destination.revision)
            || (!apply.requires_exact_save && saved != apply.original_destination)
        {
            return Err(Rejection::StaleSource);
        }
        Ok(saved)
    }

    pub(super) fn finish_prefab_apply(&mut self, apply: StartedApply) -> Result<State, Failure> {
        if !apply.matches_retained_inputs(self) {
            return Err(Rejection::StaleSource.into());
        }
        let saved = self.saved_apply_destination(&apply)?;
        let StartedApply {
            basis,
            containing,
            original_destination,
            destination,
            ..
        } = apply;
        require_confirmed_apply_inputs(self, &basis, &saved)?;
        let mut inputs = self
            .authoring_project()
            .ok_or(Rejection::UnsupportedMode)?
            .admit_source_capture_inputs(0, 0)
            .map_err(|error| source_failure(error, Rejection::ValidationFailed))?;
        let observation = self
            .capture_authoring_source_observation(&saved, &mut inputs)
            .map_err(|error| source_failure(error, Rejection::StaleSource))?;
        basis
            .sources
            .validate_after_prefab_apply(
                basis
                    .additional
                    .as_ref()
                    .and_then(|additional| additional.sources.as_ref()),
                original_destination
                    .receipt
                    .as_ref()
                    .ok_or(Rejection::StaleSource)?,
                saved.receipt.as_ref().ok_or(Rejection::StaleSource)?,
                &observation,
            )
            .map_err(|error| source_failure(error, Rejection::ValidationFailed))?;
        let document = containing.document();
        let revision = applied_revision(self.commit_authoring_edit(containing))?;
        Ok(State::Terminal(Terminal::Applied {
            inspection: self.prefab_inspection.id(),
            document,
            revision,
            destination,
        }))
    }
}

fn require_confirmed_apply_inputs(
    editor: &EditorProjectSession,
    basis: &InspectionBasis,
    destination: &AuthoringSourceSelection,
) -> Result<(), Rejection> {
    if destination.has_confirmed_checkpoint(editor)
        && basis
            .selection
            .apply_inputs_are_saved(editor, destination.document)
        && basis.additional.as_ref().is_none_or(|candidate| {
            candidate
                .selection
                .apply_inputs_are_saved(editor, destination.document)
        })
    {
        Ok(())
    } else {
        Err(Rejection::ApplySourceSaveRequired)
    }
}

fn applied_revision(
    outcome: EditorActionOutcome,
) -> Result<nara_scene::SceneAuthoringRevision, Failure> {
    match outcome {
        EditorActionOutcome::Workspace(report) if report.applied => report
            .revision
            .ok_or_else(|| Rejection::SourceRejected.into()),
        EditorActionOutcome::Workspace(report) => Err(Failure {
            reason: Rejection::SourceRejected,
            diagnostics: report.diagnostics,
        }),
        _ => Err(Rejection::SourceRejected.into()),
    }
}

fn apply_rejection(error: AuthoringPrefabPropertyApplyError) -> Rejection {
    match error {
        AuthoringPrefabPropertyApplyError::Read(error) => property_read_rejection(error),
        AuthoringPrefabPropertyApplyError::InvalidDestination => Rejection::InvalidDestination,
        AuthoringPrefabPropertyApplyError::InvalidProperty => Rejection::InvalidProperty,
        AuthoringPrefabPropertyApplyError::BudgetExceeded => Rejection::BudgetExceeded,
    }
}
