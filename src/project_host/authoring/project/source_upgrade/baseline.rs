//! One finite capture of the exact Workspace inputs and complete physical source inventory.
//!
//! The Session admits and freezes its inputs before calling this trusted private owner. Capture
//! transfers that allowance and the original source Work; it does not construct another Workspace,
//! reclassify working sources or acquire a second preparation inventory.

use nara_scene::__private::AuthoredSchemaUpgradeUsage;
use nara_scene::{SceneFileEncoding, SourceDocumentKey};

use super::*;
use crate::project_host::authoring::source_inventory::SourceEncoding;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

/// Every open slot participates, including an explicitly unbound or unobservable slot.
#[derive(Debug)]
pub(crate) struct SourceUpgradeOpenInput {
    pub(crate) document: EditorDocumentId,
    pub(crate) source: Option<AuthoringDocumentSourceFact>,
    pub(crate) observation: Option<FileSourceObservation>,
}

impl SourceUpgradeOpenInput {
    /// Admit this storage before copying the selection. Observation acquisition is admitted
    /// separately by `AuthoringSourceCaptureInputs::capture_source_observation`.
    pub(crate) fn capture_retained_bytes(
        source: Option<&AuthoringDocumentSourceFact>,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        checked_sum([
            size_of::<Self>(),
            source.map_or(Ok(0), |source| {
                source
                    .checked_retained_bytes()
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
            })?,
        ])
    }
}

/// Move-only request for a finite worker, without live Session or Workspace mutation authority.
pub(crate) struct SourceUpgradeCaptureRequest {
    pub(super) schema: Arc<AuthoringSchemaReceipt>,
    pub(super) basis: AuthoringSchemaBasis,
    pub(super) scene_limits: SceneFileLimits,
    pub(super) budget: AuthoringProjectAdmissionBudget,
    pub(super) work: AuthoredSchemaUpgradeWork,
    pub(super) input: EditorWorkspaceAuthoredSchemaUpgradeInput,
    pub(super) open: Vec<SourceUpgradeOpenInput>,
}

/// The same owned baseline is later consumed by proposal preparation.
pub(crate) struct SourceUpgradeBaseline {
    pub(super) schema: Arc<AuthoringSchemaReceipt>,
    pub(super) basis: AuthoringSchemaBasis,
    pub(super) scene_limits: SceneFileLimits,
    pub(super) budget: AuthoringProjectAdmissionBudget,
    pub(super) work: AuthoredSchemaUpgradeWork,
    pub(super) input: EditorWorkspaceAuthoredSchemaUpgradeInput,
    pub(super) open: Vec<SourceUpgradeOpenInput>,
    pub(super) inventory: SourceInventory,
}

/// Borrowed command input. An unopened source uses its original disk bytes; an open source
/// additionally supplies its exact frozen working document, including unsaved edits.
pub(crate) struct SourceUpgradeSourceInput<'a> {
    pub(crate) key: &'a SourceDocumentKey,
    pub(crate) path: &'a str,
    pub(crate) encoding: SceneFileEncoding,
    pub(crate) disk: &'a [u8],
    pub(crate) document: Option<EditorDocumentId>,
    pub(crate) scene: Option<&'a SceneDocument>,
    pub(crate) prefab: Option<&'a PrefabDocument>,
}

impl AuthoringProjectSessionSnapshot {
    /// Transfers the already admitted Host/worker selections and their live observation proofs.
    ///
    /// Before cloning selections, the caller must admit their storage with
    /// `admit_source_operation_inputs`, using `SourceUpgradeOpenInput::capture_retained_bytes` for
    /// each worker selection, plus `size_of::<SourceUpgradeCaptureRequest>()`. Acquire each proof
    /// through those same inputs. Capture the complete Workspace with the supplied Work before
    /// transferring both here. Host selection retention, worker inputs and observation handles
    /// remain charged while the baseline retains them.
    pub(crate) fn capture_authored_upgrade_request(
        &self,
        input: EditorWorkspaceAuthoredSchemaUpgradeInput,
        mut open: Vec<SourceUpgradeOpenInput>,
        mut inputs: AuthoringSourceCaptureInputs,
        work: AuthoredSchemaUpgradeWork,
    ) -> Result<SourceUpgradeCaptureRequest, SourceUpgradePreparationError> {
        let selected_bytes = open.iter().try_fold(0, |bytes, selected| {
            checked_sum([
                bytes,
                SourceUpgradeOpenInput::capture_retained_bytes(selected.source.as_ref())?,
            ])
        })?;
        inputs
            .worker_retained_bytes
            .checked_sub(checked_sum([
                selected_bytes,
                size_of::<SourceUpgradeCaptureRequest>(),
            ])?)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        // The inherited root allowance owns the manifest/bundle bodies. It does not include
        // this admitted nominal basis or the shared receipt/control-block storage. Reserve
        // those before retaining either handle. Scene Work excludes shared Schema catalogs.
        let metadata = checked_sum([
            checked_product(
                self.schema.bundle_bytes().len(),
                AUTHORING_BASIS_ENCODED_MULTIPLIER,
            )?,
            size_of::<AuthoringSchemaReceipt>(),
            4 * size_of::<usize>(),
        ])?;
        inputs
            .budget
            .charge_source_bytes(0, 0, metadata, metadata)?;
        let basis = self.startup_scene.schema_basis();
        if inputs.basis != basis
            || input.schema_basis() != &basis
            || input.documents().len() != open.len()
            || open
                .iter()
                .any(|selected| selected.source.is_none() || selected.observation.is_none())
        {
            return Err(SourceUpgradePreparationError::OpenSet);
        }
        inputs
            .observation_count
            .checked_sub(open.len())
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        let comparisons = (usize::BITS - open.len().leading_zeros()) as usize + 1;
        inputs.budget.charge_source_bytes(
            0,
            0,
            checked_product(selected_bytes, checked_product(comparisons, 2)?)?,
            0,
        )?;

        // Workspace capture orders document IDs. In-place sorting proves exact membership
        // without copying source facts or allocating another complete open-set table.
        open.sort_unstable_by_key(|selected| selected.document);
        for (selected, document) in open.iter().zip(input.documents()) {
            let source = selected
                .source
                .as_ref()
                .ok_or(SourceUpgradePreparationError::OpenSet)?;
            let observation = selected
                .observation
                .as_ref()
                .ok_or(SourceUpgradePreparationError::OpenSet)?;
            let kind_matches = match source.source_key().kind() {
                SourceDocumentKind::Scene => input.scene_document(document).is_some(),
                SourceDocumentKind::Prefab => input.prefab_document(document).is_some(),
            };
            if selected.document != document
                || !kind_matches
                || input.document_source_binding(document).is_none()
                || !capture::source_locator_matches(
                    input.document_source(document),
                    source.relative_path(),
                )
            {
                return Err(SourceUpgradePreparationError::OpenSet);
            }
            capture::source_observations::check_document_observation(source, observation)?;
        }
        open.sort_unstable_by(|left, right| {
            left.source
                .as_ref()
                .map(AuthoringDocumentSourceFact::relative_path)
                .cmp(
                    &right
                        .source
                        .as_ref()
                        .map(AuthoringDocumentSourceFact::relative_path),
                )
        });
        if open.windows(2).any(|pair| {
            pair[0]
                .source
                .as_ref()
                .map(AuthoringDocumentSourceFact::relative_path)
                == pair[1]
                    .source
                    .as_ref()
                    .map(AuthoringDocumentSourceFact::relative_path)
        }) {
            return Err(SourceUpgradePreparationError::OpenSet);
        }
        Ok(SourceUpgradeCaptureRequest {
            schema: Arc::clone(&self.schema),
            basis,
            scene_limits: self.scene_limits,
            budget: inputs.budget,
            work,
            input,
            open,
        })
    }
}

impl SourceUpgradeCaptureRequest {
    /// Captures exactly one complete inventory on the inherited allowance. Refusal drops this
    /// operation's inputs and custody; no finite task or publication survives the returned error.
    pub(crate) fn capture(self) -> Result<SourceUpgradeBaseline, SourceUpgradePreparationError> {
        let Self {
            schema,
            basis,
            scene_limits,
            mut budget,
            work,
            input,
            open,
        } = self;
        let reference_bytes = checked_product(open.len(), size_of::<OpenSource<'_>>())?;
        budget.charge_source_bytes(0, 0, reference_bytes, reference_bytes)?;
        let sources = open
            .iter()
            .map(|selected| {
                selected
                    .source
                    .as_ref()
                    .map(OpenSource::Bound)
                    .ok_or(SourceUpgradePreparationError::OpenSet)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let captured = SourceInventory::capture(&schema, &sources, scene_limits, &mut budget);
        drop(sources);
        budget.reconcile_retained_reservation(reference_bytes, 0)?;
        let inventory = captured?;
        validate_observations(&inventory, &open, &mut budget)?;
        Ok(SourceUpgradeBaseline {
            schema,
            basis,
            scene_limits,
            budget,
            work,
            input,
            open,
            inventory,
        })
    }
}

fn validate_observations(
    inventory: &SourceInventory,
    open: &[SourceUpgradeOpenInput],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradePreparationError> {
    let custody = inventory.shared_custody();
    let mut participants = inventory.participants();
    for selected in open {
        let source = selected
            .source
            .as_ref()
            .ok_or(SourceUpgradePreparationError::OpenSet)?;
        let observation = selected
            .observation
            .as_ref()
            .ok_or(SourceUpgradePreparationError::OpenSet)?;
        // Both sets are ordered by path. This borrows the one original inventory, including
        // the actual digest read by it, rather than validating copied file IDs alone.
        let member = loop {
            let member = participants
                .next()
                .ok_or(SourceInventoryError::OpenSourceChanged)?;
            budget.charge_source_bytes(
                0,
                0,
                checked_sum([
                    member.file().path().len(),
                    source.relative_path().len(),
                    size_of::<(FileIdentity, Option<FileIdentity>, ContentDigest)>(),
                ])?,
                0,
            )?;
            if member.file().path() == source.relative_path() {
                break member;
            }
        };
        let file = custody
            .file(member.file().path())
            .ok_or(SourceInventoryError::OpenSourceChanged)?;
        if observation.observation()
            != (
                member.file().identity(),
                member.file().parent_identity(),
                member.file().digest(),
            )
            || !observation
                .matches(file)
                .map_err(SourceInventoryError::Host)?
        {
            return Err(SourceInventoryError::OpenSourceChanged.into());
        }
    }
    Ok(())
}

impl SourceUpgradeBaseline {
    pub(crate) fn bundle(&self) -> &AuthoringSchemaBundle {
        self.schema.bundle()
    }

    pub(crate) fn source_count(&self) -> usize {
        self.inventory.participants().len()
    }

    pub(crate) fn sources(
        &self,
    ) -> impl ExactSizeIterator<Item = SourceUpgradeSourceInput<'_>> + '_ {
        self.inventory.participants().map(|member| {
            let document = self
                .open
                .binary_search_by(|selected| {
                    selected
                        .source
                        .as_ref()
                        .map(AuthoringDocumentSourceFact::relative_path)
                        .cmp(&Some(member.file().path()))
                })
                .ok()
                .map(|index| self.open[index].document);
            SourceUpgradeSourceInput {
                key: member.source_key(),
                path: member.file().path(),
                encoding: match member.encoding() {
                    SourceEncoding::Json => SceneFileEncoding::Json,
                    SourceEncoding::Ron => SceneFileEncoding::Ron,
                },
                disk: member.bytes(),
                document,
                scene: document.and_then(|document| self.input.scene_document(document)),
                prefab: document.and_then(|document| self.input.prefab_document(document)),
            }
        })
    }

    pub(crate) fn open_count(&self) -> usize {
        self.open.len()
    }

    /// Exact in-memory Schema authority and previous nominal basis, not equal disk bytes.
    pub(crate) fn matches(&self, project: &AuthoringProjectSessionSnapshot) -> bool {
        Arc::ptr_eq(&self.schema, &project.schema)
            && self.basis == project.startup_scene.schema_basis()
    }

    pub(crate) fn input(&self) -> &EditorWorkspaceAuthoredSchemaUpgradeInput {
        &self.input
    }

    pub(super) fn inventory(&self) -> &SourceInventory {
        &self.inventory
    }

    pub(crate) fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.budget.usage()
    }

    pub(crate) fn work_usage(&self) -> AuthoredSchemaUpgradeUsage {
        self.work.usage()
    }
}
