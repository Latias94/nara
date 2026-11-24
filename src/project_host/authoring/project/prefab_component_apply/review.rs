use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, SafeSummary};
use nara_scene::{
    __private::{
        PrefabComponentApplyPreparationError, PrefabComponentApplyRequest,
        PreparedPrefabComponentApply, try_prepare_prefab_component_apply_with_loader,
    },
    AuthoringPrefabInstanceSnapshot, PrefabDeclarationLimits, PrefabEntityTarget, ScenePatchReport,
};
use nara_tooling::{
    EditorPrefabComponentAddress, EditorWorkspaceCommandReport,
    EditorWorkspacePrefabComponentApplyInput, EncodedEditorPersistenceCheckpoint,
    PreparedEditorWorkspacePrefabComponentApply,
};

use super::{
    super::super::{document::AuthoringDocumentSourceFact, source_inventory::OpenSource},
    *,
};

pub(crate) struct ComponentApplyProjectReviewInput {
    schema: Arc<AuthoringSchemaReceipt>,
    basis: AuthoringSchemaBasis,
    limits: SceneFileLimits,
    budget: AuthoringProjectAdmissionBudget,
    open: Vec<Option<AuthoringDocumentSourceFact>>,
    replaced: [(AuthoringDocumentSourceFact, FileSourceObservation); 2],
}

pub(crate) struct ComponentApplyProjectReview {
    pub(super) schema: Arc<AuthoringSchemaReceipt>,
    pub(super) inventory: SourceInventory,
    pub(super) budget: AuthoringProjectAdmissionBudget,
    pub(super) workspace: PreparedEditorWorkspacePrefabComponentApply,
    pub(super) outputs: [EncodedEditorPersistenceCheckpoint; 2],
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ComponentApplyProjectReviewError {
    #[error("a replaced source no longer matches its saved observation")]
    StaleSource,
    #[error("the admitted source inventory changed")]
    Inventory(#[from] SourceInventoryError),
    #[error("the paired saved generation cannot be published")]
    SavedGeneration(#[from] ComponentApplySavedGenerationError),
    #[error("the original selected declaration and destination cannot prepare an Apply pair")]
    Pair(PrefabComponentApplyPreparationError<AuthoringProjectAdmissionError>),
    #[error("the paired Workspace outputs cannot be prepared")]
    Workspace(Box<EditorWorkspaceCommandReport>),
    #[error("the paired source capture exceeds its Host allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("native source observations could not be retained")]
    SourceCapture(#[from] AuthoringProjectSourceAdmissionError),
    #[error("a retained source cannot be revalidated")]
    Physical(#[from] FsError),
}

impl AuthoringProjectSessionSnapshot {
    pub(crate) fn capture_component_apply_project_review(
        &self,
        inputs: AuthoringSourceCaptureInputs,
        open: Vec<Option<AuthoringDocumentSourceFact>>,
        replaced: [(AuthoringDocumentSourceFact, FileSourceObservation); 2],
    ) -> ComponentApplyProjectReviewInput {
        ComponentApplyProjectReviewInput {
            schema: Arc::clone(&self.schema),
            basis: inputs.basis,
            limits: self.scene_limits,
            budget: inputs.budget,
            open,
            replaced,
        }
    }
}

impl ComponentApplyProjectReviewInput {
    fn capture_inventory(&mut self) -> Result<SourceInventory, ComponentApplyProjectReviewError> {
        use ComponentApplyProjectReviewError as Error;
        let inventory = {
            let scratch = self
                .open
                .len()
                .checked_mul(size_of::<OpenSource>())
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            self.budget.charge_source_bytes(0, 0, scratch, scratch)?;
            let open = open_sources(&self.open);
            let result =
                SourceInventory::capture(&self.schema, &open, self.limits, &mut self.budget);
            drop(open);
            self.budget.reconcile_retained_reservation(scratch, 0)?;
            result?
        };
        for (source, observation) in &self.replaced {
            let member = inventory
                .participant(source.relative_path())
                .ok_or(Error::StaleSource)?;
            let captured = member.file();
            let file = inventory
                .shared_file(captured.path())
                .ok_or(Error::StaleSource)?;
            if member.source_key() != source.source_key()
                || observation.observation()
                    != (
                        captured.identity(),
                        captured.parent_identity(),
                        captured.digest(),
                    )
                || !observation.matches(&file)?
            {
                return Err(Error::StaleSource);
            }
        }
        Ok(inventory)
    }

    pub(crate) fn prepare_selected(
        mut self,
        workspace: EditorWorkspacePrefabComponentApplyInput,
        inspection: &AuthoringPrefabInstanceSnapshot,
        address: &EditorPrefabComponentAddress,
        destination_route: &PrefabEntityTarget,
        declaration_limits: PrefabDeclarationLimits,
    ) -> Result<ComponentApplyProjectReview, ComponentApplyProjectReviewError> {
        use ComponentApplyProjectReviewError as Error;
        let inventory = self.capture_inventory()?;
        let root = self.schema.project_root();
        let sources = workspace.source_pair();
        let retained = self.budget.retained_candidate_bytes;
        let request = PrefabComponentApplyRequest {
            sources,
            inspection,
            selected: &address.entity,
            destination_route,
            component: &address.component,
            declaration_limits,
            compilation: SceneCompilationOptions::default(),
        };
        let mut saved = capture_saved_sources(
            &inventory,
            &self.basis,
            &[
                ComponentApplyReplacement {
                    path: self.replaced[0].0.relative_path(),
                    key: sources.containing_key(),
                    body: match sources.shared_containing_scene() {
                        Some(document) => ComponentApplySourceBody::Scene(document),
                        None => ComponentApplySourceBody::Prefab(
                            sources
                                .shared_containing_prefab()
                                .ok_or(Error::StaleSource)?,
                        ),
                    },
                },
                ComponentApplyReplacement {
                    path: self.replaced[1].0.relative_path(),
                    key: sources.destination_key(),
                    body: ComponentApplySourceBody::Prefab(sources.shared_destination()),
                },
            ],
            &mut self.budget,
        )?;
        let index = inventory
            .configurations()
            .iter()
            .position(|configuration| configuration.profile_name() == inventory.selected_profile())
            .ok_or(Error::StaleSource)?;
        let mut prepared = None;
        let mut work = SavedGenerationWork::default();
        with_temporary_storage(&mut self.budget, |budget| {
            let environment = SavedEnvironment::capture(&root, &inventory, index, &saved, budget)?;
            let compilation = work.options(budget)?;
            let pair = try_prepare_prefab_component_apply_with_loader(
                PrefabComponentApplyRequest {
                    compilation,
                    ..request
                },
                |reference, working| environment.load(&saved, reference, working, budget),
            )
            .map_err(Error::Pair)?;
            let bytes = compilation
                .limits
                .working_bytes()
                .min(remaining_saved_host_work(budget)?)
                .get()
                .checked_sub(pair.remaining_work_bytes())
                .ok_or(SourceInventoryError::AccountingOverflow)?;
            work.settle(budget, Some(bytes), pair.reference_work())?;
            budget.charge_source_bytes(0, 0, 0, bytes)?;
            for (output, slot) in saved.replacements.into_iter().enumerate() {
                let entry = &mut saved.entries[slot];
                match &mut entry.body {
                    SavedSourceBody::Scene(document) if output == 0 => {
                        *document = pair.shared_containing_scene().ok_or(Error::StaleSource)?;
                    }
                    SavedSourceBody::Prefab(source) => {
                        let document = if output == 0 {
                            pair.shared_containing_prefab().ok_or(Error::StaleSource)?
                        } else {
                            pair.shared_destination()
                        };
                        *source = saved_prefab(
                            &inventory,
                            &self.basis,
                            entry.path,
                            source.key(),
                            document,
                            budget,
                        )?;
                    }
                    SavedSourceBody::Scene(_) => return Err(Error::StaleSource),
                }
            }
            environment.validate(&saved, &self.basis, &mut work, budget)?;
            prepared = Some((pair, bytes));
            Ok::<(), Error>(())
        })?;
        let (pair, bytes) = prepared.expect("successful review prepares the captured pair");
        self.budget.charge_source_bytes(0, 0, 0, bytes)?;
        validate_saved_sources(
            &root,
            &inventory,
            &self.basis,
            &saved,
            Some(index),
            &mut work,
            &mut self.budget,
        )?;
        drop(saved);
        let temporary = self
            .budget
            .retained_candidate_bytes
            .checked_sub(retained)
            .and_then(|temporary| temporary.checked_sub(bytes))
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        self.budget.reconcile_retained_reservation(temporary, 0)?;
        self.finish(inventory, workspace, pair)
    }

    fn finish(
        mut self,
        inventory: SourceInventory,
        workspace: EditorWorkspacePrefabComponentApplyInput,
        pair: PreparedPrefabComponentApply,
    ) -> Result<ComponentApplyProjectReview, ComponentApplyProjectReviewError> {
        let (workspace, outputs) = workspace
            .prepare_persistence(pair, |bytes| {
                admit_persistence_work(&mut self.budget, bytes)
            })
            .map_err(|failure| {
                let (_, _, report) = failure.into_parts();
                ComponentApplyProjectReviewError::Workspace(Box::new(report))
            })?;
        Ok(ComponentApplyProjectReview {
            schema: self.schema,
            inventory,
            budget: self.budget,
            workspace,
            outputs,
        })
    }
}

impl ComponentApplyProjectReview {
    pub(crate) fn workspace(&self) -> &PreparedEditorWorkspacePrefabComponentApply {
        &self.workspace
    }

    pub(crate) fn outputs(&self) -> &[EncodedEditorPersistenceCheckpoint; 2] {
        &self.outputs
    }

    pub(crate) fn original_bytes(&self, path: &str) -> &[u8] {
        self.inventory
            .participant(path)
            .expect("review only exposes its two admitted source paths")
            .bytes()
    }

    #[cfg(test)]
    pub(crate) fn revalidate_project(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        current_open: &[Option<AuthoringDocumentSourceFact>],
    ) -> Result<(), ComponentApplyProjectReviewError> {
        with_temporary_storage(&mut self.budget, |budget| {
            let scratch = current_open
                .len()
                .checked_mul(size_of::<OpenSource>())
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            budget.charge_source_bytes(0, 0, scratch, scratch)?;
            self.inventory
                .validate(&project.schema, &open_sources(current_open), budget)?;
            Ok(())
        })
    }
}

pub(super) fn open_sources(open: &[Option<AuthoringDocumentSourceFact>]) -> Vec<OpenSource<'_>> {
    open.iter()
        .map(|entry| match entry {
            Some(source) => OpenSource::Bound(source),
            None => OpenSource::Unbound,
        })
        .collect()
}

impl AuthoringSourceCaptureInputs {
    pub(crate) fn admit_component_apply_workspace_work(
        &mut self,
        bytes: Option<usize>,
    ) -> Result<(), ScenePatchReport> {
        admit_persistence_work(&mut self.budget, bytes)
    }
}

fn admit_persistence_work(
    budget: &mut AuthoringProjectAdmissionBudget,
    bytes: Option<usize>,
) -> Result<(), ScenePatchReport> {
    let admitted =
        bytes.is_some_and(|bytes| budget.charge_source_bytes(0, 0, bytes, bytes).is_ok());
    if admitted {
        return Ok(());
    }
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(
        DiagnosticCode::new("project.component-apply-retention-limit")
            .expect("valid diagnostic code"),
        SafeSummary::new("Component Apply exceeds the admitted project work or storage allowance")
            .expect("valid diagnostic summary"),
    ));
    Err(ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    })
}
