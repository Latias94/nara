//! Bounded, complete data proposals joined to the one captured upgrade baseline.
//!
//! Queue admission precedes task custody. The worker reuses recovery, Schema and source owners;
//! it neither rescans preparation inputs nor interprets a project command's conversion semantics.

use nara_project::{
    DEFAULT_MANIFEST_BYTE_LIMIT, ProjectManifest, ProjectManifestWriteError, ProjectProfileOverlay,
};
use nara_reflect::{
    AuthoringSchemaBundleLimits, ComponentFieldId, ComponentPropertyPath, ComponentTypeId,
};
use nara_scene::SourceDocumentKey;

use super::*;
use crate::project_host::authoring::{
    AuthoredSchemaUpgradeHistoryPolicy,
    source_inventory::{SourceEncoding, SourceRootObservation},
    source_upgrade_ready::SourceUpgradeReadyError,
    source_upgrade_recovery::{
        SourceUpgradeRecoveryError, SourceUpgradeRecoveryInput, SourceUpgradeRecoveryLimits,
        SourceUpgradeRecoveryMemberInput, SourceUpgradeRecoveryMetadataInput,
        SourceUpgradeRecoveryRootInput,
    },
};
use crate::scene_retention::{property_path_retained_bytes, string_retained_bytes};

/// A project command's data proposal, without file, root, Schema or publication authority.
#[derive(Debug)]
pub struct AuthoredSchemaUpgradeProposal {
    pub successor_bundle: Box<[u8]>,
    pub component: ComponentTypeId,
    pub field: ComponentFieldId,
    pub successor_property: ComponentPropertyPath,
    pub sources: Box<[AuthoredSchemaUpgradeSourceProposal]>,
}

/// Exact reviewed working bytes and their proposed successor for one captured source.
///
/// The actual inventory supplies encoding, old disk bytes and complete membership.
#[derive(Debug)]
pub struct AuthoredSchemaUpgradeSourceProposal {
    pub key: SourceDocumentKey,
    pub project_path: Box<str>,
    pub reviewed_working: Box<[u8]>,
    pub successor: Box<[u8]>,
}

/// Already admitted data and its original baseline, ready for one finite task.
pub(crate) struct SourceUpgradeProposalRequest {
    baseline: SourceUpgradeBaseline,
    proposal: AuthoredSchemaUpgradeProposal,
    proposal_retained: usize,
    retained_before: usize,
    handles_before: usize,
}

/// The original baseline survives refusal, including its nominal Work and retained file custody.
pub(crate) struct SourceUpgradeProposalFailure {
    pub(crate) baseline: SourceUpgradeBaseline,
    pub(crate) error: SourceUpgradeProposalError,
}

impl std::fmt::Debug for SourceUpgradeProposalFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceUpgradeProposalFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// One actual successor admission and its complete source/Workspace preparation.
pub(crate) struct PreparedAuthoredSchemaUpgradeReview {
    pub(super) selected: PreparedSourceUpgradeSchema,
    pub(super) sources: PreparedAuthoredSourceUpgrade,
    pub(super) budget: AuthoringProjectAdmissionBudget,
    pub(super) work: AuthoredSchemaUpgradeWork,
    pub(super) open: Vec<SourceUpgradeOpenInput>,
}

/// Borrowed complete review of the exact prepared operation, without private recovery authority.
#[derive(Clone, Copy)]
pub struct AuthoredSchemaUpgradeReview<'a> {
    prepared: &'a PreparedAuthoredSchemaUpgradeReview,
}

impl std::fmt::Debug for AuthoredSchemaUpgradeReview<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthoredSchemaUpgradeReview")
            .field("component", &self.component())
            .field("field", &self.field())
            .field("successor_property", &self.successor_property())
            .field("source_count", &self.source_count())
            .field("document_count", &self.document_count())
            .field("history_policy", &self.history_policy())
            .finish_non_exhaustive()
    }
}

/// One member of the actual ordered recovery set, borrowing its immutable reviewed bytes.
#[derive(Debug, Clone, Copy)]
pub struct AuthoredSchemaUpgradeReviewedSource<'a> {
    pub key: &'a SourceDocumentKey,
    pub project_path: &'a str,
    pub original_disk: &'a [u8],
    pub reviewed_working: &'a [u8],
    pub successor: &'a [u8],
    pub document: Option<EditorDocumentId>,
}

struct ReviewedSources<'a> {
    members: std::slice::Iter<'a, RecoveryMember>,
    opened: std::iter::Peekable<std::slice::Iter<'a, OpenDocument>>,
}

impl<'a> Iterator for ReviewedSources<'a> {
    type Item = AuthoredSchemaUpgradeReviewedSource<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let member = self.members.next()?;
        let document = if self
            .opened
            .peek()
            .is_some_and(|open| open.source.relative_path() == member.path)
        {
            self.opened.next().map(|open| open.document)
        } else {
            None
        };
        Some(AuthoredSchemaUpgradeReviewedSource {
            key: &member.key,
            project_path: &member.path,
            original_disk: member.old_disk.bytes(),
            reviewed_working: member.reviewed_working.bytes(),
            successor: member.successor.bytes(),
            document,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.members.size_hint()
    }
}

impl ExactSizeIterator for ReviewedSources<'_> {}

impl<'a> AuthoredSchemaUpgradeReview<'a> {
    pub fn schema_bundle(&self) -> &'a AuthoringSchemaBundle {
        self.prepared.sources.schema().bundle()
    }

    pub fn successor_schema_bundle(&self) -> &'a AuthoringSchemaBundle {
        self.prepared.selected.bundle()
    }

    pub fn component(&self) -> &'a ComponentTypeId {
        self.prepared.sources.recovery().field_upgrade().0
    }

    pub fn field(&self) -> &'a ComponentFieldId {
        self.prepared.sources.recovery().field_upgrade().1
    }

    pub fn successor_property(&self) -> &'a ComponentPropertyPath {
        self.prepared.sources.recovery().field_upgrade().2
    }

    pub fn source_count(&self) -> usize {
        self.prepared.sources()
    }

    pub fn document_count(&self) -> usize {
        self.prepared.documents()
    }

    pub const fn history_policy(&self) -> AuthoredSchemaUpgradeHistoryPolicy {
        AuthoredSchemaUpgradeHistoryPolicy::SaveReviewedWorkingCopiesAndStartNewHistory
    }

    pub fn sources(
        &self,
    ) -> impl ExactSizeIterator<Item = AuthoredSchemaUpgradeReviewedSource<'a>> + 'a {
        let prepared = self.prepared;
        // Both owning sets are ordered by canonical project path and were joined by preparation.
        // Each open member advances once; there is no cloning or repeated nth/source scan.
        // Keep the merge in next(), so default nth/last also advance the open cursor through
        // every skipped member. A stateful Map closure would not observe those skipped members.
        ReviewedSources {
            members: prepared.sources.recovery().members().iter(),
            opened: prepared.sources.open.iter().peekable(),
        }
    }
}

impl SourceUpgradeBaseline {
    /// Admission precedes allocation of the shared Task slot and its callbacks. Failed
    /// admission releases only new storage; performed work remains cumulative.
    pub(crate) fn admit_preparation_task_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), SourceUpgradeProposalError> {
        let before = self.budget.retained_candidate_bytes;
        let result = self.budget.charge_source_bytes(0, 0, bytes, bytes);
        if result.is_err() {
            self.release_preparation_task_storage(self.budget.retained_candidate_bytes - before);
        }
        result.map_err(|error| ProposalErrorKind::Admission(error).into())
    }

    /// Called after the Task slot and its callback captures have actually dropped.
    pub(crate) fn release_preparation_task_storage(&mut self, bytes: usize) {
        self.budget
            .reconcile_retained_reservation(bytes, 0)
            .expect("only dropped proposal or Task storage is released");
    }

    /// Admits all proposal bodies and request storage before a task may retain this owner.
    /// Counts, exact source membership and bounded metadata are checked without filesystem IO.
    pub(crate) fn proposal_request(
        mut self,
        mut proposal: AuthoredSchemaUpgradeProposal,
    ) -> Result<SourceUpgradeProposalRequest, SourceUpgradeProposalFailure> {
        let retained_before = self.budget.retained_candidate_bytes;
        let handles_before = self.budget.retained_open_handles;
        let result = admit_proposal(
            &mut proposal,
            &self.inventory,
            self.scene_limits,
            &mut self.budget,
        );
        match result {
            Ok(proposal_retained) => Ok(SourceUpgradeProposalRequest {
                baseline: self,
                proposal,
                proposal_retained,
                retained_before,
                handles_before,
            }),
            Err(error) => {
                drop(proposal);
                let error = settle_refusal(&mut self.budget, retained_before, handles_before)
                    .err()
                    .unwrap_or(error);
                Err(SourceUpgradeProposalFailure {
                    baseline: self,
                    error,
                })
            }
        }
    }
}

impl SourceUpgradeProposalRequest {
    /// Keeps the captured baseline outside the disposable callback. The callback owns only
    /// admitted proposal data; an unstarted Task can return the same baseline without recapture.
    pub(crate) fn into_task_parts(
        self,
    ) -> (
        SourceUpgradeBaseline,
        usize,
        impl FnOnce(
            (SourceUpgradeBaseline, usize),
        )
            -> Result<PreparedAuthoredSchemaUpgradeReview, SourceUpgradeProposalFailure>
        + Send
        + 'static,
    ) {
        let Self {
            baseline,
            proposal,
            proposal_retained,
            retained_before,
            handles_before,
        } = self;
        let prepare = move |(baseline, task_storage): (SourceUpgradeBaseline, usize)| {
            Self {
                baseline,
                proposal,
                proposal_retained,
                // A refused preparation must keep its still-live Task storage charged.
                // The caller releases that reservation only after physical return.
                retained_before: retained_before
                    .checked_add(task_storage)
                    .expect("admitted Task storage fits the retained allowance"),
                handles_before,
            }
            .prepare()
        };
        (baseline, proposal_retained, prepare)
    }

    /// Prepares the admitted proposal on the original cumulative allowances. This is the only
    /// production preparation path; a request never acquires another baseline or conversion owner.
    pub(crate) fn prepare(
        self,
    ) -> Result<PreparedAuthoredSchemaUpgradeReview, SourceUpgradeProposalFailure> {
        let Self {
            baseline,
            proposal,
            proposal_retained,
            retained_before,
            handles_before,
        } = self;
        let SourceUpgradeBaseline {
            schema,
            basis,
            scene_limits,
            mut budget,
            mut work,
            input,
            open,
            inventory,
        } = baseline;
        let metadata = prepare_metadata(
            &schema,
            &basis,
            &inventory,
            scene_limits,
            &proposal,
            &mut budget,
        );
        // Recovery owns its own admitted copies. Raw command bytes and queued-request storage
        // end before source preparation; neither may remain charged as permanent review storage.
        drop(proposal);
        budget
            .reconcile_retained_reservation(proposal_retained, 0)
            .expect("the admitted proposal remains charged until its owned data drops");
        let mut inputs = Some((inventory, input));
        let result = work.with_preparation_attempt(|work| {
            let (selected, recovery) = metadata?;
            let reference_bytes =
                checked_product(open.len(), size_of::<SourceUpgradeOpenDocument<'_>>())?;
            budget.charge_source_bytes(
                0,
                0,
                reference_bytes,
                checked_sum([
                    reference_bytes,
                    size_of::<PreparedAuthoredSchemaUpgradeReview>(),
                ])?,
            )?;
            let complete_open: Vec<_> = open
                .iter()
                .map(|source| SourceUpgradeOpenDocument {
                    document: source.document,
                    source: source.source.as_ref(),
                })
                .collect();
            let (inventory, input) = inputs.take().expect("one original preparation input owner");
            let prepared = PreparedAuthoredSourceUpgrade::prepare(
                &schema,
                &selected,
                &recovery,
                inventory,
                input,
                &complete_open,
                &mut budget,
                work,
            );
            drop(complete_open);
            budget
                .reconcile_retained_reservation(reference_bytes, 0)
                .expect("the admitted borrowed source references have dropped");
            match prepared {
                Ok(sources) => Ok((selected, sources)),
                Err(failure) => {
                    inputs = Some((failure.inventory, failure.input));
                    Err(SourceUpgradeProposalError::from(failure.error))
                }
            }
        });
        match result {
            Ok((selected, sources)) => Ok(PreparedAuthoredSchemaUpgradeReview {
                selected,
                sources,
                budget,
                work,
                open,
            }),
            Err(error) => {
                // All attempted recovery/Schema/source preparations and borrowed vectors have
                // dropped. Keep the original inventory, Workspace inputs, work and peak facts.
                let error = settle_refusal(&mut budget, retained_before, handles_before)
                    .err()
                    .unwrap_or(error);
                let (inventory, input) = inputs.expect("refusal returns original source inputs");
                Err(SourceUpgradeProposalFailure {
                    baseline: SourceUpgradeBaseline {
                        schema,
                        basis,
                        scene_limits,
                        budget,
                        work,
                        input,
                        open,
                        inventory,
                    },
                    error,
                })
            }
        }
    }
}

impl PreparedAuthoredSchemaUpgradeReview {
    pub(crate) fn release_preparation_task_storage(&mut self, bytes: usize) {
        self.budget
            .reconcile_retained_reservation(bytes, 0)
            .expect("the completed preparation Task and callback have dropped");
    }

    pub(crate) fn review(&self) -> AuthoredSchemaUpgradeReview<'_> {
        AuthoredSchemaUpgradeReview { prepared: self }
    }

    pub(crate) fn sources(&self) -> usize {
        self.sources.recovery().members().len()
    }

    pub(crate) fn documents(&self) -> usize {
        self.sources.open.len()
    }

    /// Memory-only worker-return check. Equal bundle bytes never replace the original receipt
    /// or the actual previous nominal basis retained by the source preparation.
    pub(crate) fn matches(&self, project: &AuthoringProjectSessionSnapshot) -> bool {
        Arc::ptr_eq(self.sources.schema(), &project.schema)
            && self.sources.previous == project.startup_scene.schema_basis()
    }

    /// Confirmation additionally checks the actual complete physical set and live Workspace.
    pub(crate) fn validate(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        workspace: &EditorWorkspace,
    ) -> Result<(), SourceUpgradeProposalError> {
        if !self.matches(project) {
            return Err(ProposalErrorKind::ProjectChanged.into());
        }
        let references =
            checked_product(self.open.len(), size_of::<SourceUpgradeOpenDocument<'_>>())?;
        let retained_before = self.budget.retained_candidate_bytes;
        let result = (|| {
            self.budget
                .charge_source_bytes(0, 0, references, references)?;
            let open: Vec<_> = self
                .open
                .iter()
                .map(|source| SourceUpgradeOpenDocument {
                    document: source.document,
                    source: source.source.as_ref(),
                })
                .collect();
            self.sources
                .validate(
                    &project.schema,
                    workspace,
                    &open,
                    &mut self.budget,
                    &mut self.work,
                )
                .map_err(SourceUpgradeProposalError::from)
        })();
        let temporary = self
            .budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        self.budget.reconcile_retained_reservation(temporary, 0)?;
        result
    }
}

fn admit_proposal(
    proposal: &mut AuthoredSchemaUpgradeProposal,
    inventory: &SourceInventory,
    scene_limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<usize, SourceUpgradeProposalError> {
    if proposal.sources.len() != inventory.participants().len() {
        return Err(ProposalErrorKind::Membership.into());
    }
    let limits = SourceUpgradeRecoveryLimits::default();
    if proposal.sources.len() > limits.sources.get()
        || proposal.successor_bundle.len()
            > AuthoringSchemaBundleLimits::default().encoded_bytes().get()
    {
        return Err(ProposalErrorKind::EncodedLimit.into());
    }
    // Selector structure is finite before inspecting its strings or letting it enter a queue.
    if proposal.successor_property.steps.len() > ComponentPropertyPath::MAX_STEPS {
        return Err(ProposalErrorKind::Selector.into());
    }
    let property_bytes = property_path_retained_bytes(&proposal.successor_property)
        .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    let mut retained = checked_sum([
        size_of::<SourceUpgradeProposalRequest>(),
        checked_product(
            proposal.sources.len(),
            size_of::<AuthoredSchemaUpgradeSourceProposal>(),
        )?,
        proposal.successor_bundle.len(),
        string_retained_bytes(proposal.component.as_str())
            .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        string_retained_bytes(proposal.field.as_str())
            .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        property_bytes,
    ])?;
    let mut ordering = 0_usize;
    for source in proposal.sources.iter() {
        if source.reviewed_working.len() > scene_limits.encoded_bytes().get()
            || source.successor.len() > scene_limits.encoded_bytes().get()
        {
            return Err(ProposalErrorKind::EncodedLimit.into());
        }
        let metadata = checked_sum([source.project_path.len(), source.key.retained_bytes()])?;
        retained = checked_sum([
            retained,
            metadata,
            source.reviewed_working.len(),
            source.successor.len(),
        ])?;
        ordering = checked_sum([ordering, metadata])?;
    }
    // Raw bodies are retained by the request, independently of the later recovery's copies.
    // Admission also bounds all comparison/sort work before it observes authored key contents.
    let comparisons = (usize::BITS - proposal.sources.len().leading_zeros()) as usize + 2;
    budget.charge_source_bytes(
        0,
        0,
        checked_sum([
            retained,
            checked_product(ordering, checked_product(comparisons, 2)?)?,
        ])?,
        retained,
    )?;
    proposal
        .successor_property
        .validate()
        .map_err(|_| ProposalErrorKind::Selector)?;
    proposal
        .sources
        .sort_unstable_by(|left, right| left.project_path.cmp(&right.project_path));
    for (proposed, actual) in proposal.sources.iter().zip(inventory.participants()) {
        if proposed.project_path.as_ref() != actual.file().path()
            || &proposed.key != actual.source_key()
        {
            return Err(ProposalErrorKind::Membership.into());
        }
    }
    Ok(retained)
}

fn prepare_metadata(
    schema: &Arc<AuthoringSchemaReceipt>,
    basis: &AuthoringSchemaBasis,
    inventory: &SourceInventory,
    scene_limits: SceneFileLimits,
    proposal: &AuthoredSchemaUpgradeProposal,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(PreparedSourceUpgradeSchema, Arc<SourceUpgradeRecovery>), SourceUpgradeProposalError> {
    let ready = canonical_ready_manifest(schema, budget)?;
    let references = checked_sum([
        checked_product(
            inventory.participants().len(),
            size_of::<SourceUpgradeRecoveryMemberInput<'_>>(),
        )?,
        checked_product(
            inventory.metadata().len(),
            size_of::<SourceUpgradeRecoveryMetadataInput<'_>>(),
        )?,
        checked_product(
            inventory.roots().len(),
            size_of::<SourceUpgradeRecoveryRootInput<'_>>(),
        )?,
        checked_product(inventory.missing_sidecars().len(), size_of::<&str>())?,
    ])?;
    budget.charge_source_bytes(0, 0, references, references)?;
    let recovered = (|| {
        let members: Vec<_> = inventory
            .participants()
            .zip(proposal.sources.iter())
            .map(|(actual, proposed)| SourceUpgradeRecoveryMemberInput {
                path: actual.file().path(),
                key: actual.source_key(),
                encoding: match actual.encoding() {
                    SourceEncoding::Json => RecoverySourceEncoding::Json,
                    SourceEncoding::Ron => RecoverySourceEncoding::Ron,
                },
                explicit_native: actual.explicit_native(),
                old_disk: actual.bytes(),
                reviewed_working: &proposed.reviewed_working,
                successor: &proposed.successor,
            })
            .collect();
        let metadata: Vec<_> = inventory
            .metadata()
            .map(|actual| SourceUpgradeRecoveryMetadataInput {
                path: actual.file().path(),
                observed_bytes: actual.bytes(),
            })
            .collect();
        let roots: Vec<_> = inventory
            .roots()
            .map(|actual| SourceUpgradeRecoveryRootInput {
                path: actual.path(),
                missing_ancestor: match actual.observation() {
                    SourceRootObservation::Available { .. } => None,
                    SourceRootObservation::Missing { ancestor_path, .. } => Some(ancestor_path),
                },
            })
            .collect();
        let missing: Vec<_> = inventory.missing_sidecars().collect();
        SourceUpgradeRecovery::prepare(
            SourceUpgradeRecoveryInput {
                selected_profile: inventory.selected_profile(),
                old_ready_manifest: schema.manifest_bytes(),
                future_ready_manifest: &ready,
                bundle_path: schema.bundle_path().as_str(),
                old_bundle: schema.bundle_bytes(),
                successor_bundle: &proposal.successor_bundle,
                component: &proposal.component,
                field: &proposal.field,
                successor_property: &proposal.successor_property,
                members: &members,
                metadata: &metadata,
                roots: &roots,
                missing_sidecars: &missing,
            },
            SourceUpgradeRecoveryLimits {
                scene: scene_limits,
                ..Default::default()
            },
            budget,
        )
    })();
    let ready_bytes = ready.len();
    drop(ready);
    // The borrowed vectors inside the recovery call and this encoded manifest have ended.
    budget.reconcile_retained_reservation(checked_sum([references, ready_bytes])?, 0)?;
    let recovery = recovered?
        .into_shared(budget)
        .map_err(|failure| failure.error)?;
    let selected = PreparedSourceUpgradeSchema::prepare(schema, basis, &recovery, budget)?;
    Ok((selected, recovery))
}

fn canonical_ready_manifest(
    schema: &AuthoringSchemaReceipt,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<u8>, SourceUpgradeProposalError> {
    // This is the owning bounded parser and complete encoder over the captured bytes. It does
    // not read a new manifest or guess configuration from effective-profile settings.
    let encoded = schema.manifest_bytes().len();
    let maximum = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
        .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    let nodes = encoded.min(4_096);
    let reservation = checked_sum([
        checked_product(encoded, 8)?,
        checked_product(
            nodes,
            size_of::<ProjectProfileOverlay>() + size_of::<EffectiveProjectSettings>(),
        )?,
        size_of::<ProjectManifest>(),
        checked_product(maximum, 2)?,
    ])?;
    budget.charge_source_bytes(0, 0, checked_product(encoded, 4)?, reservation)?;
    let result = (|| {
        let load = ProjectManifest::parse_toml_bytes(schema.manifest_bytes());
        if load.has_errors() {
            return Err(ProposalErrorKind::ManifestInvalid.into());
        }
        let manifest = load.manifest.ok_or(ProposalErrorKind::ManifestInvalid)?;
        // Encoder validation, ordering, quoting and the bounded output all belong to nara_project.
        budget.charge_source_bytes(0, 0, checked_product(reservation, 2)?, 0)?;
        manifest
            .to_canonical_toml_bytes()
            .map_err(SourceUpgradeProposalError::from)
    })();
    let actual = result.as_ref().map_or(0, Vec::len);
    budget.reconcile_retained_reservation(reservation, actual)?;
    result
}

fn settle_refusal(
    budget: &mut AuthoringProjectAdmissionBudget,
    retained_before: usize,
    handles_before: usize,
) -> Result<(), SourceUpgradeProposalError> {
    let released = budget
        .retained_candidate_bytes
        .checked_sub(retained_before)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    budget.reconcile_retained_reservation(released, 0)?;
    let handles = budget
        .retained_open_handles
        .checked_sub(handles_before)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    budget.release_open_handles(handles);
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub(crate) struct SourceUpgradeProposalError(ProposalErrorKind);

#[derive(Debug, thiserror::Error)]
enum ProposalErrorKind {
    #[error("the proposal does not contain the exact captured source keys and project paths")]
    Membership,
    #[error("the proposal exceeds the owning bundle, source or complete-set limits")]
    EncodedLimit,
    #[error("the proposed successor property selector is invalid")]
    Selector,
    #[error("the captured complete manifest is invalid")]
    ManifestInvalid,
    #[error("the actual project receipt or previous nominal basis changed")]
    ProjectChanged,
    #[error("complete-proposal storage or work admission refused: {0}")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("complete-proposal accounting failed: {0}")]
    Project(#[from] AuthoringProjectSourceAdmissionError),
    #[error("canonical Ready manifest encoding failed: {0}")]
    Manifest(#[from] ProjectManifestWriteError),
    #[error("recovery data admission refused: {0}")]
    Recovery(#[from] SourceUpgradeRecoveryError),
    #[error("successor Schema admission refused: {0}")]
    Schema(#[from] SourceUpgradeReadyError),
    #[error("complete source preparation refused: {0}")]
    Sources(#[from] SourceUpgradePreparationError),
}

impl SourceUpgradeProposalError {
    pub(crate) fn host_work_limit_exceeded(&self) -> bool {
        use AuthoringProjectAdmissionError::DecodeWorkExceeded;
        match &self.0 {
            ProposalErrorKind::Admission(DecodeWorkExceeded { .. })
            | ProposalErrorKind::Project(AuthoringProjectSourceAdmissionError::Budget(
                DecodeWorkExceeded { .. },
            ))
            | ProposalErrorKind::Recovery(SourceUpgradeRecoveryError::Admission(
                DecodeWorkExceeded { .. },
            ))
            | ProposalErrorKind::Schema(SourceUpgradeReadyError::Admission(DecodeWorkExceeded {
                ..
            })) => true,
            ProposalErrorKind::Sources(error) => error.host_work_limit_exceeded(),
            _ => false,
        }
    }

    /// Borrows only real owning diagnostics. Format/membership errors without a report return
    /// None; the caller may present their error text without fabricating source observations.
    pub(crate) fn diagnostics(&self) -> Option<&DiagnosticReport> {
        match &self.0 {
            ProposalErrorKind::Schema(SourceUpgradeReadyError::Upgrade(report)) => {
                Some(&report.diagnostics)
            }
            ProposalErrorKind::Schema(SourceUpgradeReadyError::Basis(error)) => {
                Some(error.diagnostics())
            }
            ProposalErrorKind::Sources(SourceUpgradePreparationError::Source(report)) => {
                Some(&report.diagnostics)
            }
            ProposalErrorKind::Sources(SourceUpgradePreparationError::Workspace(report)) => {
                Some(&report.diagnostics)
            }
            ProposalErrorKind::Sources(SourceUpgradePreparationError::Structure(error)) => {
                Some(error.diagnostics())
            }
            ProposalErrorKind::Sources(SourceUpgradePreparationError::Document(
                document::AuthoringDocumentDataError::Publication(error),
            )) => Some(error.diagnostics()),
            ProposalErrorKind::Manifest(ProjectManifestWriteError::Invalid { diagnostics }) => {
                Some(diagnostics)
            }
            _ => None,
        }
    }
}

impl From<ProposalErrorKind> for SourceUpgradeProposalError {
    fn from(error: ProposalErrorKind) -> Self {
        Self(error)
    }
}

impl From<AuthoringProjectAdmissionError> for SourceUpgradeProposalError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self(ProposalErrorKind::Admission(error))
    }
}

impl From<AuthoringProjectSourceAdmissionError> for SourceUpgradeProposalError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        Self(ProposalErrorKind::Project(error))
    }
}

impl From<ProjectManifestWriteError> for SourceUpgradeProposalError {
    fn from(error: ProjectManifestWriteError) -> Self {
        Self(ProposalErrorKind::Manifest(error))
    }
}

impl From<SourceUpgradeRecoveryError> for SourceUpgradeProposalError {
    fn from(error: SourceUpgradeRecoveryError) -> Self {
        Self(ProposalErrorKind::Recovery(error))
    }
}

impl From<SourceUpgradeReadyError> for SourceUpgradeProposalError {
    fn from(error: SourceUpgradeReadyError) -> Self {
        Self(ProposalErrorKind::Schema(error))
    }
}

impl From<SourceUpgradePreparationError> for SourceUpgradeProposalError {
    fn from(error: SourceUpgradePreparationError) -> Self {
        Self(ProposalErrorKind::Sources(error))
    }
}
