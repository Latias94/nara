//! Complete live-source proof for one reviewed authored-field upgrade.
//!
//! Recovery data describes proposed bytes. This owner joins those bytes to the actual all-profile
//! inventory and every open working copy, then asks the Scene/Prefab and Workspace owners to
//! prove the transition. It neither publishes files nor grants a Ready project receipt. The
//! enclosing operation supplies the actual selected metadata admission, owns both cumulative
//! preparation allowances, and binds persistence, execution retirement and final publication.

use std::collections::BTreeMap;

use nara_scene::{
    __private::{
        AuthoredSchemaUpgradeWork, PreparedPrefabAuthoredSchemaUpgrade,
        PreparedSceneAuthoredSchemaUpgrade, RetainedAuthoredSchemaUpgradeSource,
        admit_authored_upgrade_prefab_source, admit_authored_upgrade_scene_source,
    },
    PrefabAuthoringSession, SceneAuthoringSession, SceneDocumentCandidate, ScenePatchReport,
    SourceDocumentKind,
};
use nara_tooling::{
    EditorAuthoredSchemaUpgradeProposal, EditorDocumentId, EditorWorkspace,
    EditorWorkspaceAuthoredSchemaUpgradeInput, EditorWorkspaceCommandReport,
    PreparedEditorWorkspaceAuthoredSchemaUpgrade,
};

use super::super::{
    document::{self, AuthoringDocumentSourceFact},
    source_upgrade_inventory::{
        SourceUpgradeInventory, SourceUpgradeInventoryError, SourceUpgradeOpenSource,
    },
    source_upgrade_ready::PreparedSourceUpgradeSchema,
    source_upgrade_recovery::{RecoveryMember, RecoverySourceEncoding, SourceUpgradeRecovery},
};
use super::*;

mod baseline;
pub(crate) use baseline::{
    SourceUpgradeBaseline, SourceUpgradeCaptureRequest, SourceUpgradeOpenInput,
};
mod proposal;
mod publication;
pub use proposal::{
    AuthoredSchemaUpgradeProposal, AuthoredSchemaUpgradeReview,
    AuthoredSchemaUpgradeReviewedSource, AuthoredSchemaUpgradeSourceProposal,
};
pub(crate) use proposal::{
    PreparedAuthoredSchemaUpgradeReview, SourceUpgradeProposalError, SourceUpgradeProposalFailure,
};
pub(crate) use publication::{
    AuthoredSourceUpgradeObservedFile, AuthoredSourceUpgradePublication,
    AuthoredSourceUpgradePublicationFailure, AuthoredSourceUpgradePublicationPhase,
};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

/// The Session supplies every open slot. An unbound or unsupported slot cannot disappear from
/// the reviewed set merely because it has no file observation.
#[derive(Clone, Copy)]
pub(crate) struct SourceUpgradeOpenDocument<'a> {
    pub(crate) document: EditorDocumentId,
    pub(crate) source: Option<&'a AuthoringDocumentSourceFact>,
}

struct OpenDocument {
    document: EditorDocumentId,
    source: AuthoringDocumentSourceFact,
}

/// Source/history successors only. Actual saved leaves and Ready adoption remain unbound.
pub(crate) struct PreparedAuthoredSourceUpgrade {
    schema: Arc<AuthoringSchemaReceipt>,
    inventory: SourceUpgradeInventory,
    input: EditorWorkspaceAuthoredSchemaUpgradeInput,
    recovery: Arc<SourceUpgradeRecovery>,
    workspace: PreparedEditorWorkspaceAuthoredSchemaUpgrade,
    unopened: Vec<PreparedUnopenedSourceUpgrade>,
    open: Vec<OpenDocument>,
    previous: AuthoringSchemaBasis,
}

/// Refusal keeps the reviewed corpus, frozen Workspace inputs and their original charged custody.
/// The caller may correct the proposal without reading another baseline or refunding live inputs.
pub(in crate::project_host::authoring) struct SourceUpgradePreparationFailure {
    pub(in crate::project_host::authoring) inventory: SourceUpgradeInventory,
    pub(in crate::project_host::authoring) input: EditorWorkspaceAuthoredSchemaUpgradeInput,
    pub(in crate::project_host::authoring) error: SourceUpgradePreparationError,
}

impl std::fmt::Debug for SourceUpgradePreparationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceUpgradePreparationFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

struct PreparedSourceContents {
    workspace: PreparedEditorWorkspaceAuthoredSchemaUpgrade,
    unopened: Vec<PreparedUnopenedSourceUpgrade>,
    open: Vec<OpenDocument>,
}

pub(crate) enum PreparedUnopenedSourceUpgrade {
    Scene {
        original: SceneAuthoringSession,
        prepared: PreparedSceneAuthoredSchemaUpgrade,
    },
    Prefab {
        original: PrefabAuthoringSession,
        prepared: PreparedPrefabAuthoredSchemaUpgrade,
    },
}

enum UnopenedIncumbent {
    Scene(SceneAuthoringSession, RetainedAuthoredSchemaUpgradeSource),
    Prefab(PrefabAuthoringSession, RetainedAuthoredSchemaUpgradeSource),
}

impl PreparedAuthoredSourceUpgrade {
    /// Moves the actual Workspace preparation while keeping every original proof alive.
    /// The final owner may put it back after a refused Ready publication.
    pub(crate) fn into_install_parts(
        self,
    ) -> (
        PreparedEditorWorkspaceAuthoredSchemaUpgrade,
        AuthoredSourceUpgradeInstallRetention,
    ) {
        let Self {
            schema,
            inventory,
            input,
            recovery,
            workspace,
            unopened,
            open,
            previous,
        } = self;
        (
            workspace,
            AuthoredSourceUpgradeInstallRetention {
                schema,
                inventory,
                input,
                recovery,
                unopened,
                open,
                previous,
            },
        )
    }

    pub(crate) fn from_install_parts(
        workspace: PreparedEditorWorkspaceAuthoredSchemaUpgrade,
        retained: AuthoredSourceUpgradeInstallRetention,
    ) -> Self {
        let AuthoredSourceUpgradeInstallRetention {
            schema,
            inventory,
            input,
            recovery,
            unopened,
            open,
            previous,
        } = retained;
        Self {
            schema,
            inventory,
            input,
            recovery,
            workspace,
            unopened,
            open,
            previous,
        }
    }

    /// Uses the metadata owner's actual bundle/Adapter admission. Recovery DATA alone never
    /// establishes the nominal authority required by a live source checkpoint.
    /// The inventory and frozen Workspace are the exact inputs exposed to the project command.
    /// Preparation consumes them only on success; refusal returns both original owners unchanged.
    /// Schema/recovery Arcs remain borrowed and do not gain a second retained-body allowance.
    pub(in crate::project_host::authoring) fn prepare(
        schema: &Arc<AuthoringSchemaReceipt>,
        selected: &PreparedSourceUpgradeSchema,
        recovery: &Arc<SourceUpgradeRecovery>,
        inventory: SourceUpgradeInventory,
        input: EditorWorkspaceAuthoredSchemaUpgradeInput,
        complete_open: &[SourceUpgradeOpenDocument<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<Self, SourceUpgradePreparationFailure> {
        let retained = budget.retained_candidate_bytes;
        let handles = budget.retained_open_handles;
        let result = work.with_preparation_attempt(|work| {
            Self::prepare_sources(
                schema,
                selected,
                recovery,
                &inventory,
                &input,
                complete_open,
                budget,
                work,
            )
        });
        match result {
            Ok(contents) => Ok(Self {
                schema: Arc::clone(schema),
                inventory,
                input,
                recovery: Arc::clone(recovery),
                workspace: contents.workspace,
                unopened: contents.unopened,
                open: contents.open,
                previous: selected.upgrade().previous_basis().clone(),
            }),
            Err(error) => {
                // Candidate locals have dropped. The returned inventory and borrowed inputs
                // retain their original reservations; only this attempt's new storage ends.
                let settled: Result<(), SourceUpgradePreparationError> = (|| {
                    let released = budget
                        .retained_candidate_bytes
                        .checked_sub(retained)
                        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
                    budget.reconcile_retained_reservation(released, 0)?;
                    let closed = budget
                        .retained_open_handles
                        .checked_sub(handles)
                        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
                    budget.release_open_handles(closed);
                    Ok(())
                })();
                Err(SourceUpgradePreparationFailure {
                    inventory,
                    input,
                    error: settled.err().unwrap_or(error),
                })
            }
        }
    }

    fn prepare_sources(
        schema: &Arc<AuthoringSchemaReceipt>,
        selected: &PreparedSourceUpgradeSchema,
        recovery: &Arc<SourceUpgradeRecovery>,
        inventory: &SourceUpgradeInventory,
        workspace: &EditorWorkspaceAuthoredSchemaUpgradeInput,
        complete_open: &[SourceUpgradeOpenDocument<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<PreparedSourceContents, SourceUpgradePreparationError> {
        if selected
            .incumbent()
            .is_none_or(|incumbent| !Arc::ptr_eq(schema, incumbent))
            || selected.operation() != recovery.digest()
        {
            return Err(SourceUpgradePreparationError::RecoveryInventory);
        }
        let upgrade = selected.upgrade();
        let open =
            admit_frozen_open_set(workspace, complete_open, upgrade.previous_basis(), budget)?;
        let inventory_inputs = inventory_inputs(complete_open, budget)?;
        let inventory_result =
            inventory.validate_preparation_inputs(schema, &inventory_inputs, budget);
        let input_bytes = checked_product(
            inventory_inputs.len(),
            size_of::<SourceUpgradeOpenSource<'_>>(),
        )?;
        drop(inventory_inputs);
        budget.reconcile_retained_reservation(input_bytes, 0)?;
        inventory_result?;
        inventory.validate_original_recovery(&schema, &recovery, budget)?;
        let scene_limits = inventory.scene_limits();
        let slots = checked_product(
            inventory.participants().len(),
            size_of::<(usize, UnopenedIncumbent)>()
                + size_of::<PreparedUnopenedSourceUpgrade>()
                + size_of::<EditorAuthoredSchemaUpgradeProposal>(),
        )?;
        budget.charge_source_bytes(0, 0, 0, checked_sum([slots, size_of::<Self>()])?)?;
        // The owned Workspace input already retained every incumbent before the project command.
        // Keep that same snapshot and nominal Work while unopened classifications are prepared.
        let mut unopened = Vec::with_capacity(inventory.participants().len());
        let mut proposals = Vec::with_capacity(open.len());

        // Open originals are already retained. Classify and retain every unopened incumbent
        // before any source in either group prepares its successor.
        for (index, member) in recovery.members().iter().enumerate() {
            let selected = open
                .binary_search_by(|source| source.source.relative_path().cmp(&member.path))
                .ok()
                .map(|index| &open[index]);
            if let Some(selected) = selected {
                validate_working_copy(workspace, selected, member, scene_limits, budget)?;
                let tree = decode_source(member, member.successor.bytes(), scene_limits, budget)?;
                let binding = workspace
                    .document_source_binding(selected.document)
                    .ok_or(SourceUpgradePreparationError::OpenSet)?;
                let metadata = checked_sum([member.key.retained_bytes(), binding.locator().len()])?;
                budget.charge_source_bytes(0, 0, metadata, metadata)?;
                let binding = binding.clone();
                let revision = workspace
                    .document_revision(selected.document)
                    .ok_or(SourceUpgradePreparationError::OpenSet)?;
                let proposal = match tree {
                    SourceTree::Scene(document) => EditorAuthoredSchemaUpgradeProposal::scene(
                        selected.document,
                        member.key.clone(),
                        binding,
                        revision,
                        SceneDocumentCandidate::from_source_document(document, scene_limits)?,
                    ),
                    SourceTree::Prefab(document) => EditorAuthoredSchemaUpgradeProposal::prefab(
                        selected.document,
                        member.key.clone(),
                        binding,
                        revision,
                        PrefabDocumentCandidate::from_source_document(document, scene_limits)?,
                    ),
                };
                proposals.push(proposal);
            } else {
                // A live review cannot introduce a dirty working copy for a closed source.
                // Fresh-process recovery has its own explicit restoration/adoption operation.
                let tree = decode_source(member, member.old_disk.bytes(), scene_limits, budget)?;
                let reviewed = decode_source(
                    member,
                    member.reviewed_working.bytes(),
                    scene_limits,
                    budget,
                )?;
                let same = match (&tree, &reviewed) {
                    (SourceTree::Scene(left), SourceTree::Scene(right)) => left == right,
                    (SourceTree::Prefab(left), SourceTree::Prefab(right)) => left == right,
                    _ => false,
                };
                release_tree(reviewed, budget)?;
                if !same {
                    return Err(SourceUpgradePreparationError::WorkingCopy);
                }
                let retained = tree.retained_bytes()?;
                budget.charge_source_bytes(
                    0,
                    0,
                    member.path.len(),
                    checked_product(member.path.len(), 2)?,
                )?;
                let source = document::document_source(&ProjectPath::new(member.path.clone())?)
                    .map_err(SourceUpgradePreparationError::Document)?;
                let result = match tree {
                    SourceTree::Scene(document) => admit_authored_upgrade_scene_source(
                        SceneDocumentCandidate::from_source_document(document, scene_limits)?,
                        source,
                        upgrade.previous_basis(),
                        work,
                    )
                    .map(|(session, retained)| UnopenedIncumbent::Scene(session, retained)),
                    SourceTree::Prefab(document) => admit_authored_upgrade_prefab_source(
                        PrefabDocumentCandidate::from_source_document(document, scene_limits)?,
                        source,
                        upgrade.previous_basis(),
                        work,
                    )
                    .map(|(session, retained)| UnopenedIncumbent::Prefab(session, retained)),
                };
                // The Scene Work now owns this source allocation, including refusal disposal.
                budget.reconcile_retained_reservation(retained, 0)?;
                unopened.push((
                    index,
                    result.map_err(SourceUpgradePreparationError::Source)?,
                ));
            }
        }
        let workspace = workspace
            .prepare(upgrade, proposals, work)
            .map_err(SourceUpgradePreparationError::Workspace)?;
        let mut prepared_unopened = Vec::with_capacity(unopened.len());
        for (index, incumbent) in unopened {
            let member = &recovery.members()[index];
            let tree = decode_source(member, member.successor.bytes(), scene_limits, budget)?;
            let prepared = match (incumbent, tree) {
                (UnopenedIncumbent::Scene(original, retained), SourceTree::Scene(document)) => {
                    let prepared = original
                        .prepare_authored_upgrade_with_retained_source(
                            &member.key,
                            original.revision(),
                            SceneDocumentCandidate::from_source_document(document, scene_limits)?,
                            upgrade,
                            retained,
                            work,
                        )
                        .map_err(SourceUpgradePreparationError::Source)?;
                    PreparedUnopenedSourceUpgrade::Scene { original, prepared }
                }
                (UnopenedIncumbent::Prefab(original, retained), SourceTree::Prefab(document)) => {
                    let prepared = original
                        .prepare_authored_upgrade_with_retained_source(
                            &member.key,
                            original.revision(),
                            PrefabDocumentCandidate::from_source_document(document, scene_limits)?,
                            upgrade,
                            retained,
                            work,
                        )
                        .map_err(SourceUpgradePreparationError::Source)?;
                    PreparedUnopenedSourceUpgrade::Prefab { original, prepared }
                }
                _ => return Err(SourceUpgradePreparationError::RecoveryInventory),
            };
            prepared_unopened.push(prepared);
        }
        Ok(PreparedSourceContents {
            workspace,
            unopened: prepared_unopened,
            open,
        })
    }

    pub(crate) fn workspace(&self) -> &PreparedEditorWorkspaceAuthoredSchemaUpgrade {
        &self.workspace
    }

    /// Borrows the exact predecessor admitted for this operation, including an accepted Save
    /// drained before capture. The live lane's older source fact cannot replace this receipt.
    pub(crate) fn predecessor_source(
        &self,
        document: EditorDocumentId,
        project_path: &str,
    ) -> Option<&AuthoringDocumentSourceFact> {
        let index = self
            .open
            .binary_search_by(|source| source.source.relative_path().cmp(project_path))
            .ok()?;
        let source = &self.open[index];
        (source.document == document).then_some(&source.source)
    }

    pub(in crate::project_host::authoring) fn recovery(&self) -> &SourceUpgradeRecovery {
        &self.recovery
    }

    pub(in crate::project_host::authoring) fn shared_recovery(&self) -> Arc<SourceUpgradeRecovery> {
        Arc::clone(&self.recovery)
    }

    pub(in crate::project_host::authoring) fn inventory(&self) -> &SourceUpgradeInventory {
        &self.inventory
    }

    pub(crate) fn schema(&self) -> &Arc<AuthoringSchemaReceipt> {
        &self.schema
    }

    pub(crate) fn unopened(&self) -> &[PreparedUnopenedSourceUpgrade] {
        &self.unopened
    }

    /// Borrows the publication limit of the exact prepared successor corresponding to the
    /// immutable recovery member. File publishers cannot substitute a wider default policy.
    pub(crate) fn source_file_limits(&self, index: usize) -> Option<SceneFileLimits> {
        let member = self.recovery.members().get(index)?;
        if let Ok(open) = self
            .open
            .binary_search_by(|source| source.source.relative_path().cmp(&member.path))
        {
            let document = self.open[open].document;
            return match member.key.kind() {
                SourceDocumentKind::Scene => self
                    .workspace
                    .scene(document)
                    .map(|source| source.file_limits()),
                SourceDocumentKind::Prefab => self
                    .workspace
                    .prefab(document)
                    .map(|source| source.file_limits()),
            };
        }
        let index = self
            .unopened
            .binary_search_by(|source| {
                let source = match source {
                    PreparedUnopenedSourceUpgrade::Scene { prepared, .. } => prepared.source(),
                    PreparedUnopenedSourceUpgrade::Prefab { prepared, .. } => prepared.source(),
                };
                match source.as_asset_ref() {
                    AssetRef::Path(path) => path.as_str().cmp(&member.path),
                    _ => std::cmp::Ordering::Less,
                }
            })
            .ok()?;
        match &self.unopened[index] {
            PreparedUnopenedSourceUpgrade::Scene { prepared, .. } => Some(prepared.file_limits()),
            PreparedUnopenedSourceUpgrade::Prefab { prepared, .. } => Some(prepared.file_limits()),
        }
    }

    /// Confirmation still precedes Pending. The outer metadata owner separately revalidates
    /// its exact manifest/bundle publication bindings and owns the execution/persistence fence.
    pub(in crate::project_host::authoring) fn validate(
        &self,
        schema: &Arc<AuthoringSchemaReceipt>,
        workspace: &EditorWorkspace,
        complete_open: &[SourceUpgradeOpenDocument<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<(), SourceUpgradePreparationError> {
        if !Arc::ptr_eq(&self.schema, schema)
            || self.input.schema_basis() != &self.previous
            || complete_open.len() != self.open.len()
        {
            return Err(SourceUpgradePreparationError::OpenSet);
        }
        let retained_before = budget.retained_candidate_bytes;
        let result = (|| {
            let current = admit_open_set(workspace, complete_open, &self.previous, budget)?;
            for (original, current) in self.open.iter().zip(&current) {
                if original.document != current.document || original.source != current.source {
                    return Err(SourceUpgradePreparationError::OpenSet);
                }
            }
            let inputs = inventory_inputs(complete_open, budget)?;
            self.inventory.validate(schema, &inputs, budget)?;
            self.workspace
                .validate(workspace, work)
                .map_err(SourceUpgradePreparationError::Workspace)
        })();
        // All confirmation-only collections have dropped. Retain the original prepared set
        // and cumulative work; do not turn discarded scans into permanent candidate storage.
        let temporary = budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        budget.reconcile_retained_reservation(temporary, 0)?;
        result
    }
}

/// Original live sources, frozen inputs and unopened proofs stay owned across final binding.
/// No field here supplies mutation authority or a second preparation allowance.
pub(crate) struct AuthoredSourceUpgradeInstallRetention {
    schema: Arc<AuthoringSchemaReceipt>,
    inventory: SourceUpgradeInventory,
    input: EditorWorkspaceAuthoredSchemaUpgradeInput,
    recovery: Arc<SourceUpgradeRecovery>,
    unopened: Vec<PreparedUnopenedSourceUpgrade>,
    open: Vec<OpenDocument>,
    previous: AuthoringSchemaBasis,
}

fn admit_open_set(
    workspace: &EditorWorkspace,
    complete_open: &[SourceUpgradeOpenDocument<'_>],
    basis: &AuthoringSchemaBasis,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<OpenDocument>, SourceUpgradePreparationError> {
    if workspace.len() != complete_open.len() {
        return Err(SourceUpgradePreparationError::OpenSet);
    }
    charge_open_work(complete_open, budget)?;
    let mut seen = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut open = Vec::with_capacity(complete_open.len());
    for selected in complete_open {
        let source = selected
            .source
            .ok_or(SourceUpgradePreparationError::OpenSet)?;
        let (actual_basis, actual_source) = if let Some(slot) = workspace.scene(selected.document) {
            if source.source_key().kind() != SourceDocumentKind::Scene {
                return Err(SourceUpgradePreparationError::OpenSet);
            }
            (
                slot.session().authoring_schema_basis(),
                slot.session().authoring_source(),
            )
        } else if let Some(slot) = workspace.prefab(selected.document) {
            if source.source_key().kind() != SourceDocumentKind::Prefab {
                return Err(SourceUpgradePreparationError::OpenSet);
            }
            (
                slot.session().authoring_schema_basis(),
                slot.session().authoring_source(),
            )
        } else {
            return Err(SourceUpgradePreparationError::OpenSet);
        };
        if actual_basis.as_ref() != Some(basis)
            || !capture::source_locator_matches(actual_source, source.relative_path())
            || seen.insert(selected.document, ()).is_some()
            || paths.insert(source.relative_path(), ()).is_some()
            || workspace
                .document_source_binding(selected.document)
                .is_none()
        {
            return Err(SourceUpgradePreparationError::OpenSet);
        }
        open.push(OpenDocument {
            document: selected.document,
            source: source.clone(),
        });
    }
    open.sort_unstable_by(|left, right| {
        left.source
            .relative_path()
            .cmp(right.source.relative_path())
    });
    drop(seen);
    drop(paths);
    budget.reconcile_retained_reservation(checked_product(complete_open.len(), 384)?, 0)?;
    Ok(open)
}

fn admit_frozen_open_set(
    workspace: &EditorWorkspaceAuthoredSchemaUpgradeInput,
    complete_open: &[SourceUpgradeOpenDocument<'_>],
    basis: &AuthoringSchemaBasis,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<OpenDocument>, SourceUpgradePreparationError> {
    if workspace.documents().len() != complete_open.len() || workspace.schema_basis() != basis {
        return Err(SourceUpgradePreparationError::OpenSet);
    }
    charge_open_work(complete_open, budget)?;
    let mut seen = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut open = Vec::with_capacity(complete_open.len());
    for selected in complete_open {
        let source = selected
            .source
            .ok_or(SourceUpgradePreparationError::OpenSet)?;
        let actual = workspace
            .document_source(selected.document)
            .ok_or(SourceUpgradePreparationError::OpenSet)?;
        let matches_kind = match source.source_key().kind() {
            SourceDocumentKind::Scene => workspace.scene_document(selected.document).is_some(),
            SourceDocumentKind::Prefab => workspace.prefab_document(selected.document).is_some(),
        };
        if !matches_kind
            || !capture::source_locator_matches(Some(actual), source.relative_path())
            || seen.insert(selected.document, ()).is_some()
            || paths.insert(source.relative_path(), ()).is_some()
            || workspace
                .document_source_binding(selected.document)
                .is_none()
        {
            return Err(SourceUpgradePreparationError::OpenSet);
        }
        open.push(OpenDocument {
            document: selected.document,
            source: source.clone(),
        });
    }
    open.sort_unstable_by(|left, right| {
        left.source
            .relative_path()
            .cmp(right.source.relative_path())
    });
    drop((seen, paths));
    budget.reconcile_retained_reservation(checked_product(complete_open.len(), 384)?, 0)?;
    Ok(open)
}

fn charge_open_work(
    complete_open: &[SourceUpgradeOpenDocument<'_>],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradePreparationError> {
    let comparisons = (usize::BITS - complete_open.len().leading_zeros()) as usize + 1;
    let bytes = complete_open.iter().try_fold(
        checked_product(complete_open.len(), size_of::<OpenDocument>() + 384)?,
        |bytes, source| {
            checked_sum([
                bytes,
                source.source.map_or(Ok(0), |source| {
                    source
                        .checked_retained_bytes()
                        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
                })?,
            ])
        },
    )?;
    budget.charge_source_bytes(0, 0, checked_product(bytes, comparisons)?, bytes)?;
    Ok(())
}

fn inventory_inputs<'a>(
    complete_open: &[SourceUpgradeOpenDocument<'a>],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<SourceUpgradeOpenSource<'a>>, SourceUpgradePreparationError> {
    let bytes = checked_product(
        complete_open.len(),
        size_of::<SourceUpgradeOpenSource<'_>>(),
    )?;
    budget.charge_source_bytes(0, 0, bytes, bytes)?;
    Ok(complete_open
        .iter()
        .map(|source| {
            source.source.map_or(
                SourceUpgradeOpenSource::Unbound,
                SourceUpgradeOpenSource::Bound,
            )
        })
        .collect())
}

fn validate_working_copy(
    workspace: &EditorWorkspaceAuthoredSchemaUpgradeInput,
    selected: &OpenDocument,
    member: &RecoveryMember,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradePreparationError> {
    let reviewed = decode_source(member, member.reviewed_working.bytes(), limits, budget)?;
    let same = match &reviewed {
        SourceTree::Scene(document) => workspace
            .scene_document(selected.document)
            .is_some_and(|source| source == document),
        SourceTree::Prefab(document) => workspace
            .prefab_document(selected.document)
            .is_some_and(|source| source == document),
    };
    release_tree(reviewed, budget)?;
    if same {
        Ok(())
    } else {
        Err(SourceUpgradePreparationError::WorkingCopy)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceUpgradePreparationError {
    #[error("the complete open source set, binding or nominal basis changed")]
    OpenSet,
    #[error("recovery data does not describe the complete observed project inventory")]
    RecoveryInventory,
    #[error("the reviewed working copy differs from its current source")]
    WorkingCopy,
    #[error("the physical upgrade inventory could not be admitted")]
    Inventory(SourceUpgradeInventoryError),
    #[error("project preparation admission refused")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("source preparation accounting or project admission failed")]
    Project(#[from] AuthoringProjectSourceAdmissionError),
    #[error("source path is not project relative")]
    Path(#[from] ProjectPathError),
    #[error("source decoding failed")]
    Format(#[from] SceneFormatError),
    #[error("the source hierarchy is invalid")]
    Structure(#[source] SceneFilePublicationError),
    #[error("source admission failed")]
    Document(document::AuthoringDocumentDataError),
    #[error("the owning source refused its authored transition")]
    Source(ScenePatchReport),
    #[error("the Workspace refused the complete source transition")]
    Workspace(EditorWorkspaceCommandReport),
}

impl From<SourceUpgradeInventoryError> for SourceUpgradePreparationError {
    fn from(error: SourceUpgradeInventoryError) -> Self {
        Self::Inventory(error)
    }
}

impl From<SourceUpgradeDecodeError> for SourceUpgradePreparationError {
    fn from(error: SourceUpgradeDecodeError) -> Self {
        match error {
            SourceUpgradeDecodeError::Admission(error) => Self::Admission(error),
            SourceUpgradeDecodeError::Project(error) => Self::Project(error),
            SourceUpgradeDecodeError::Format(error) => Self::Format(error),
            SourceUpgradeDecodeError::Structure(error) => Self::Structure(error),
        }
    }
}

impl SourceUpgradePreparationError {
    pub(crate) fn invalid_metadata_path(&self) -> Option<&str> {
        match self {
            Self::Inventory(SourceUpgradeInventoryError::MetadataFormat { path, .. }) => Some(path),
            _ => None,
        }
    }

    pub(crate) fn host_work_limit_exceeded(&self) -> bool {
        use AuthoringProjectAdmissionError::DecodeWorkExceeded;
        matches!(
            self,
            Self::Admission(DecodeWorkExceeded { .. })
                | Self::Project(AuthoringProjectSourceAdmissionError::Budget(
                    DecodeWorkExceeded { .. }
                ))
                | Self::Inventory(SourceUpgradeInventoryError::Admission(
                    DecodeWorkExceeded { .. }
                ))
                | Self::Inventory(SourceUpgradeInventoryError::Native(
                    AuthoringProjectSourceAdmissionError::Budget(DecodeWorkExceeded { .. })
                ))
        )
    }
}
