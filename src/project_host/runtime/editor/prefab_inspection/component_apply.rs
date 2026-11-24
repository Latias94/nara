//! Whole-component promotion through the existing source inspection and project publication owners.

use nara_scene::{__private::PrefabComponentApplyPreparationError, PrefabEntityTarget};
use nara_tooling::{
    EditorPlayState, EditorPrefabComponentApplyFileReview, EditorPrefabComponentApplyReview,
    EncodedEditorPersistenceCheckpoint, PreparedEditorWorkspacePrefabComponentApply,
};

use crate::project_host::authoring::{
    AuthoringSourceOperationLimits, ComponentApplyLiveError, ComponentApplyLivePublication,
    ComponentApplyPersistenceDestination, ComponentApplyProjectReview,
    ComponentApplyProjectReviewError, ComponentApplySavedGenerationError,
    document::AuthoringDocumentSourceFact,
};

use super::super::{
    EditorExecutionLane, PrefabPersistenceHost, ScenePersistenceHost,
    authoring_work::{OwnedJob, OwnedReturn},
    persistence::RetainedHostGuard,
};
use super::*;

type PublicationJob = OwnedJob<Box<Publication>, Box<Publication>>;

pub(super) struct PreparedComponentApply {
    review: ComponentApplyProjectReview,
}

impl PreparedComponentApply {
    pub(super) fn validate_workspace(
        &self,
        editor: &EditorProjectSession,
    ) -> Result<(), Rejection> {
        self.review
            .workspace()
            .validate(&editor.workspace)
            .map_err(|_| Rejection::StaleSource)
    }

    fn view(&self) -> EditorPrefabComponentApplyReview<'_> {
        let workspace = self.review.workspace();
        let input = workspace.input();
        let pair = workspace.scene_pair();
        EditorPrefabComponentApplyReview {
            kind: pair.kind(),
            files: [
                (
                    input.containing_basis(),
                    input.containing_binding(),
                    pair.containing_key(),
                    pair.containing_patch(),
                ),
                (
                    input.destination_basis(),
                    input.destination_binding(),
                    pair.destination_key(),
                    pair.destination_patch(),
                ),
            ]
            .map(|(basis, binding, source, patch)| {
                let project_path = binding.locator();
                let index = usize::from(basis.document == input.destination_basis().document);
                EditorPrefabComponentApplyFileReview {
                    basis,
                    source,
                    project_path,
                    original_bytes: self.review.original_bytes(project_path),
                    output_bytes: self.review.outputs()[index].bytes(),
                    patch,
                }
            }),
        }
    }
}

pub(super) struct PublishingComponentApply {
    work: Option<PublicationJob>,
    owner: Option<Box<Publication>>,
    published: usize,
}

struct Publication {
    live: ComponentApplyLivePublication,
    prepared: Option<PreparedEditorWorkspacePrefabComponentApply>,
    outputs: Option<[EncodedEditorPersistenceCheckpoint; 2]>,
    failure: Option<Failure>,
}

impl Publication {
    fn advance(mut self: Box<Self>) -> Box<Self> {
        let [first, second] = self
            .outputs
            .as_ref()
            .expect("publishing retains both fixed outputs");
        self.failure = self
            .live
            .advance([first.bytes(), second.bytes()])
            .err()
            .map(publication_failure);
        self
    }
}

impl PublishingComponentApply {
    pub(super) const fn published(&self) -> usize {
        self.published
    }
}

enum PersistenceGuard<'a> {
    Scene(RetainedHostGuard<'a, ScenePersistenceHost>),
    Prefab(RetainedHostGuard<'a, PrefabPersistenceHost>),
}

impl PersistenceGuard<'_> {
    fn destination(
        &mut self,
        document: EditorDocumentId,
    ) -> ComponentApplyPersistenceDestination<'_> {
        match self {
            Self::Scene(guard) => {
                ComponentApplyPersistenceDestination::Scene(document, guard.host_mut())
            }
            Self::Prefab(guard) => {
                ComponentApplyPersistenceDestination::Prefab(document, guard.host_mut())
            }
        }
    }
}

impl EditorProjectSession {
    /// Borrows both fixed existing-file replacements and the exact edits reviewed for publication.
    pub fn prefab_component_apply_review(
        &self,
        inspection: EditorPrefabInspectionId,
    ) -> Result<EditorPrefabComponentApplyReview<'_>, Rejection> {
        self.check_prefab_inspection_id(inspection)?;
        let State::Review {
            basis,
            edit: InspectionReview::ComponentApply(edit),
        } = &self.prefab_inspection.state
        else {
            return Err(Rejection::WrongPhase);
        };
        self.check_prefab_inspection_basis(basis)?;
        edit.validate_workspace(self)?;
        Ok(edit.view())
    }

    pub(super) fn start_component_apply(
        &mut self,
        address: EditorPrefabComponentAddress,
        destination: EditorDocumentQueryBasis,
        destination_route: PrefabEntityTarget,
        limits: PrefabDeclarationLimits,
    ) -> Result<(), Rejection> {
        match self.capture_component_apply(address, destination, destination_route, limits) {
            Ok(()) => Ok(()),
            Err(failure) => {
                let _ = self.diagnostics.extend(failure.diagnostics);
                Err(failure.reason)
            }
        }
    }

    fn capture_component_apply(
        &mut self,
        address: EditorPrefabComponentAddress,
        destination: EditorDocumentQueryBasis,
        destination_route: PrefabEntityTarget,
        limits: PrefabDeclarationLimits,
    ) -> Result<(), Failure> {
        let State::Ready { basis, snapshot } = &self.prefab_inspection.state else {
            return Err(Rejection::WrongPhase.into());
        };
        self.revalidate_prefab_inspection_sources(basis)?;
        admit_prefab_component_address(&address)?;
        let route_bytes = destination_route
            .logical_bytes_up_to(MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES)
            .ok_or(Rejection::BudgetExceeded)?;
        if super::super::actions::component_uses_ui_domain(&address.component) {
            return Err(Rejection::InvalidComponent.into());
        }
        if basis.document.document == destination.document
            || self.workspace.document_kind(destination.document)
                != Some(EditorDocumentKind::Prefab)
        {
            return Err(Rejection::InvalidDestination.into());
        }
        let containing = self
            .authoring_source_selection(basis.document.document)
            .ok_or(Rejection::StaleSource)?;
        let target = self
            .authoring_source_selection(destination.document)
            .filter(|source| basis.selection.used_working_copy(source))
            .ok_or(Rejection::InvalidDestination)?;
        let replacements = [containing.document, target.document];
        if !containing.has_confirmed_checkpoint(self)
            || !target.has_confirmed_checkpoint(self)
            || !basis
                .selection
                .apply_inputs_are_saved_except(self, &replacements)
        {
            return Err(Rejection::ApplySourceSaveRequired.into());
        }
        let project = self.authoring_project().ok_or(Rejection::UnsupportedMode)?;
        let operation_limits = AuthoringSourceOperationLimits::default();
        let retained = snapshot
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(route_bytes))
            .and_then(|bytes| bytes.checked_add(address.component.as_str().len()))
            .and_then(|bytes| bytes.checked_add(MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES))
            .and_then(|bytes| bytes.checked_add(self.component_apply_open_sources_bytes()?))
            .and_then(|bytes| {
                bytes.checked_add(PublicationJob::storage_bytes(&Publication::advance))
            })
            .ok_or(Rejection::BudgetExceeded)?;
        let capture_failure = |error| source_failure(error, Rejection::PreparationFailed);
        let mut inputs = project
            .admit_source_operation_inputs(
                0,
                retained,
                operation_limits.host_work(),
                operation_limits.open_handles(),
            )
            .map_err(capture_failure)?;
        inputs
            .retain_prior_inspection(&basis.sources)
            .map_err(capture_failure)?;
        let workspace = self
            .persistence_authority
            .capture_prefab_component_apply(
                &self.workspace,
                &basis.document,
                &containing.source_key,
                &destination,
                &target.source_key,
                |bytes| inputs.admit_component_apply_workspace_work(bytes),
            )
            .map_err(workspace_failure)?;
        let first = self
            .capture_authoring_source_observation(&containing, &mut inputs)
            .map_err(capture_failure)?;
        let second = self
            .capture_authoring_source_observation(&target, &mut inputs)
            .map_err(capture_failure)?;
        let review = project.capture_component_apply_project_review(
            inputs,
            self.component_apply_open_sources(),
            [
                (containing.receipt.ok_or(Rejection::StaleSource)?, first),
                (target.receipt.ok_or(Rejection::StaleSource)?, second),
            ],
        );

        let State::Ready { basis, snapshot } = mem::take(&mut self.prefab_inspection.state) else {
            unreachable!();
        };
        let task = self
            .authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(self.prefab_inspection.sequence, INSPECTION_DOMAIN),
                move |_| {
                    let review = review
                        .prepare_selected(
                            workspace,
                            &snapshot,
                            &address,
                            &destination_route,
                            limits,
                        )
                        .map_err(review_failure)?;
                    Ok(Box::new(PreparedComponentApply { review }))
                },
            )
            .into_handle()
            .map_err(task_rejection);
        self.prefab_inspection.state = match task {
            Ok(task) => State::Working(InspectionWork::ComponentApply { basis, task }),
            Err(reason) => {
                self.prefab_inspection.state = State::Terminal(Terminal::Failed {
                    inspection: self.prefab_inspection.id(),
                    reason,
                    diagnostics: Failure::from(reason).diagnostics,
                });
                return Err(reason.into());
            }
        };
        Ok(())
    }

    fn component_apply_open_sources_bytes(&self) -> Option<usize> {
        self.component_apply_documents()
            .try_fold(0usize, |total, document| {
                total
                    .checked_add(size_of::<Option<AuthoringDocumentSourceFact>>())?
                    .checked_add(self.authoring_source_selection_retained_bytes(document)?)
            })
    }

    fn component_apply_open_sources(&self) -> Vec<Option<AuthoringDocumentSourceFact>> {
        self.component_apply_documents()
            .map(|id| {
                self.authoring_source_selection(id)
                    .and_then(|source| source.receipt)
            })
            .collect()
    }

    fn component_apply_documents(&self) -> impl Iterator<Item = EditorDocumentId> + '_ {
        self.workspace
            .__document_ids()
            .into_iter()
            .filter(|document| {
                matches!(
                    self.workspace.document_kind(*document),
                    Some(EditorDocumentKind::Scene | EditorDocumentKind::Prefab)
                )
            })
    }

    pub(super) fn require_component_apply_confirmation(
        &self,
        prepared: &PreparedComponentApply,
    ) -> Result<(), Rejection> {
        prepared.validate_workspace(self)?;
        if self.play_view().state() != EditorPlayState::Empty
            || self.external_play_request.is_some()
            || self.supervised_play_retirement_pending()
            || self.persistence.has_pending_work()
            || self.pending_document_open.is_some()
            || self.authoring_retirements.is_pending()
            || self.source_asset_import.schema_adoption_pending()
            || self.image_import.schema_adoption_import_pending()
            || self.prefab_conversion.schema_adoption_pending()
            || self
                .source_tools
                .physical_work_pending(self.authoring_runtime.host_custody())
        {
            return Err(Rejection::Busy);
        }
        Ok(())
    }

    pub(super) fn confirm_component_apply(
        &mut self,
        prepared: PreparedComponentApply,
    ) -> Result<State, Failure> {
        let project = self.authoring_project().ok_or(Rejection::UnsupportedMode)?;
        let (live, prepared, outputs) = prepared
            .review
            .begin_publication(project, self.component_apply_open_sources())
            .map_err(publication_failure)?;
        Ok(State::PublishingComponentApply(Box::new(
            PublishingComponentApply {
                owner: Some(Box::new(Publication {
                    live,
                    prepared: Some(prepared),
                    outputs: Some(outputs),
                    failure: None,
                })),
                work: None,
                published: 0,
            },
        )))
    }

    pub(super) fn advance_component_apply(
        &mut self,
        mut state: Box<PublishingComponentApply>,
    ) -> State {
        if let Some(work) = &mut state.work {
            if work.pending.poll().is_none() {
                return State::PublishingComponentApply(state);
            }
            let job = state.work.take().expect("the returned work remains owned");
            state.owner = match job.into_owner() {
                Ok(OwnedReturn::Returned(owner)) => Some(owner),
                Ok(OwnedReturn::Unstarted(mut owner)) => {
                    owner
                        .failure
                        .get_or_insert_with(|| Rejection::TaskUnavailable.into());
                    Some(owner)
                }
                Err(_) => {
                    return self.component_apply_stopped(true, Rejection::TaskCleanupFailed.into());
                }
            };
            state.published = state
                .owner
                .as_ref()
                .expect("the worker returned its owner")
                .live
                .published();
            return State::PublishingComponentApply(state);
        }
        let mut owner = state.owner.take().expect("publication has a live owner");
        state.published = owner.live.published();
        if owner.failure.is_none() && owner.live.ready_to_install() {
            match self.install_component_apply(&mut owner) {
                Ok(result) => {
                    self.prefab_inspection.component_apply_fenced = false;
                    return State::Terminal(result);
                }
                Err(failure) => owner.failure = Some(failure),
            }
        }
        if let Some(failure) = owner.failure.take() {
            let pending = owner.live.requires_recovery();
            if let Some(publisher) = owner.live.into_cleanup_publisher() {
                self.authoring_runtime
                    .host_custody()
                    .retain_publication(publisher)
                    .expect("admitted component Apply returns before file custody closes");
            }
            return self.component_apply_stopped(pending, failure);
        }
        let job = PublicationJob::start(
            self.authoring_runtime.persistence_tasks(),
            TaskSpawnRequest::new(self.prefab_inspection.sequence, INSPECTION_DOMAIN),
            owner,
            Publication::advance,
        );
        state.work = Some(job);
        State::PublishingComponentApply(state)
    }

    fn component_apply_stopped(&mut self, pending: bool, failure: Failure) -> State {
        self.prefab_inspection.component_apply_fenced = pending;
        let inspection = self.prefab_inspection.id();
        State::Terminal(if pending {
            Terminal::ComponentApplyRecoveryRequired {
                inspection,
                diagnostics: failure.diagnostics,
            }
        } else {
            Terminal::Failed {
                inspection,
                reason: failure.reason,
                diagnostics: failure.diagnostics,
            }
        })
    }

    fn install_component_apply(&mut self, owner: &mut Publication) -> Result<Terminal, Failure> {
        let prepared = owner.prepared.take().ok_or(Rejection::PublicationFailed)?;
        let encoded = owner.outputs.take().ok_or(Rejection::PublicationFailed)?;
        let input = prepared.input();
        let containing = input.containing_basis().document;
        let destination = input.destination_basis().document;
        let pair = prepared.scene_pair();
        let terminal = Terminal::ComponentApplied {
            inspection: self.prefab_inspection.id(),
            document: containing,
            revision: pair.containing_future_revision(),
            destination: EditorPrefabApplyDestination {
                document: destination,
                revision: pair.destination_future_revision(),
                changed: true,
            },
            kind: pair.kind(),
        };
        let (project, sources) = {
            let acquire = |document| {
                if let Some(lane) = self.scene_lanes.get(&document) {
                    lane.host.try_bind().map(PersistenceGuard::Scene)
                } else {
                    self.prefab_lanes
                        .get(&document)
                        .and_then(|lane| lane.host.try_bind())
                        .map(PersistenceGuard::Prefab)
                }
                .ok_or(Rejection::StaleSource)
            };
            let mut first = acquire(containing)?;
            let mut second = acquire(destination)?;
            owner
                .live
                .install(
                    &self.persistence_authority,
                    &mut self.workspace,
                    prepared,
                    encoded,
                    [
                        first.destination(containing),
                        second.destination(destination),
                    ],
                )
                .map_err(publication_failure)?
        };
        for (document, source) in [containing, destination].into_iter().zip(sources) {
            if let Some(lane) = self.scene_lanes.get_mut(&document) {
                lane.authoring_source = Some(source);
            } else {
                self.prefab_lanes
                    .get_mut(&document)
                    .expect("the original bound Prefab lane remains present")
                    .authoring_source = Some(source);
            }
        }
        self.execution = EditorExecutionLane::Authoring(project);
        self.authoring_prefab_source_observations.clear();
        self.prefab_content_refresh.clear();
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
        Ok(terminal)
    }
}

fn workspace_failure(report: nara_tooling::EditorWorkspaceCommandReport) -> Failure {
    Failure {
        reason: Rejection::PreparationFailed,
        diagnostics: report.diagnostics,
    }
}

fn review_failure(error: ComponentApplyProjectReviewError) -> Failure {
    use ComponentApplyProjectReviewError as Review;
    use PrefabComponentApplyPreparationError as Pair;
    match error {
        Review::StaleSource | Review::Pair(Pair::StaleBasis) => Rejection::StaleSource.into(),
        Review::SourceCapture(error) => source_failure(error, Rejection::PreparationFailed),
        Review::Admission(_) => Rejection::BudgetExceeded.into(),
        Review::Pair(Pair::WorkLimit) => Rejection::BudgetExceeded.into(),
        Review::Pair(Pair::InvalidComponent) => Rejection::InvalidComponent.into(),
        Review::Pair(Pair::InvalidDestination | Pair::NonPortableReference) => {
            Rejection::InvalidDestination.into()
        }
        Review::Pair(Pair::Patch(report)) => source_patch_failure(report.diagnostics),
        Review::Pair(Pair::Compilation(AuthoringPrefabInspectionError::Compilation {
            diagnostics,
            ..
        }))
        | Review::SavedGeneration(ComponentApplySavedGenerationError::Composition {
            diagnostics,
            ..
        }) => Failure {
            reason: Rejection::SourceRejected,
            diagnostics,
        },
        Review::Workspace(report) => workspace_failure(*report),
        _ => Rejection::ValidationFailed.into(),
    }
}

fn publication_failure(error: ComponentApplyLiveError) -> Failure {
    match error {
        ComponentApplyLiveError::Review(error) => review_failure(error),
        ComponentApplyLiveError::Stale | ComponentApplyLiveError::Inventory(_) => {
            Rejection::StaleSource.into()
        }
        ComponentApplyLiveError::Admission(_) => Rejection::BudgetExceeded.into(),
        _ => Failure {
            reason: Rejection::PublicationFailed,
            diagnostics: super::super::single_error(
                "project.editor.prefab-component-apply-publication",
                "The reviewed component Apply could not complete its file publication",
            ),
        },
    }
}
