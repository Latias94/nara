//! Preprepared lane facts and the original all-bound Ready publisher.

use super::*;
use crate::project_host::persistence::EditorPersistenceReceipt;

pub(crate) use super::super::publication::ExtractionPersistenceDestination as PrefabExtractionPersistenceDestination;

/// All fields were allocated before Ready. Remaining native/resource custody stays in the
/// publication facade until the caller has moved the result and refunded binding temporaries.
#[must_use]
pub(crate) struct PrefabExtractionSessionInstallation {
    pub(crate) session: AuthoringProjectSessionSnapshot,
    pub(crate) sources: Vec<(EditorDocumentId, AuthoringDocumentSourceFact)>,
    pub(crate) receipts: Vec<(EditorDocumentId, Option<EditorPersistenceReceipt>)>,
}

impl PrefabExtractionSessionPublication {
    /// Complete original lanes in document order. No allocation, lookup, clone or disk read.
    pub(crate) fn installation_documents(
        &self,
    ) -> impl ExactSizeIterator<Item = (EditorDocumentId, &AuthoringDocumentSourceFact)> + '_ {
        let open = &self.context().open;
        self.originals.iter().map(move |index| {
            let original = &open[*index];
            (
                original.document,
                original
                    .source
                    .as_ref()
                    .expect("the captured complete set is bound"),
            )
        })
    }

    /// Every original Host, lane and execution destination is held by the caller. The only
    /// durable Ready write is the existing Workspace/leaf-bound publisher below. Refusal
    /// restores its exact original prepared owner, complete evidence and cumulative allowance.
    pub(crate) fn install(
        &mut self,
        authority: &EditorPersistenceAuthority,
        workspace: &mut EditorWorkspace,
        destinations: &mut [PrefabExtractionPersistenceDestination<'_>],
    ) -> Result<PrefabExtractionSessionInstallation, ()> {
        if self.failure.is_some() || self.cancelling || self.task_storage != 0 {
            self.record_failure(Failure::Phase.into());
            return Err(());
        }
        let stage = self
            .stage
            .take()
            .expect("installation retains its exact owner");
        let Stage::Ready(prepared) = stage else {
            self.stage = Some(stage);
            self.record_failure(Failure::Phase.into());
            return Err(());
        };
        if self.sources.len() != prepared.persistence().documents().len()
            || self.originals.len() != self.sources.len()
        {
            self.stage = Some(Stage::Ready(prepared));
            self.record_failure(Failure::Phase.into());
            return Err(());
        }
        let bound = match prepared.bind_workspace(authority, workspace) {
            Ok(bound) => bound,
            Err(failure) => {
                let (prepared, report) = failure.into_parts();
                self.stage = Some(Stage::Ready(prepared));
                self.record_failure(Failure::Workspace(report).into());
                return Err(());
            }
        };
        match bound.publish(destinations) {
            Ok(installed) => {
                let (session, receipts, custody) = installed.into_session_parts();
                self.stage = Some(Stage::Installed(custody));
                Ok(PrefabExtractionSessionInstallation {
                    session,
                    sources: std::mem::take(&mut self.sources),
                    receipts,
                })
            }
            Err(failure) => {
                let (prepared, error) = failure.into_parts();
                self.stage = Some(Stage::Ready(prepared));
                self.record_failure(Failure::Installation(error).into());
                Err(())
            }
        }
    }
}

pub(super) fn prepare_sources(
    sources: &mut Vec<(EditorDocumentId, AuthoringDocumentSourceFact)>,
    originals: &mut Vec<usize>,
    future: &mut ProjectPrefabExtractionFutureInventory,
) -> Result<(), PrefabExtractionSessionPublicationError> {
    let count = future.publication().session_context().open.len();
    if sources.len() == count && originals.len() == count {
        return Ok(());
    }
    if sources.capacity() == 0 && count != 0 {
        let slots = checked_product(
            count,
            checked_sum([
                size_of::<(EditorDocumentId, AuthoringDocumentSourceFact)>(),
                size_of::<usize>(),
            ])?,
        )?;
        admission::admit_storage(
            &mut future.publication_mut().session_context_mut().budget,
            slots,
        )?;
        *sources = Vec::with_capacity(count);
        *originals = Vec::with_capacity(count);
    }
    // The immutable inventory and original context are distinct exact owners. This borrowed
    // split creates no cloned inventory or new reservation.
    let (inventory, context) = future.session_inputs_mut();
    let comparisons = (usize::BITS - count.leading_zeros()) as usize + 1;
    context.budget.charge_source_bytes(
        0,
        0,
        checked_product(count, checked_product(comparisons, 2)?)?,
        0,
    )?;
    for index in sources.len()..context.open.len() {
        let original = &context.open[index];
        let source = original.source.as_ref().ok_or(Failure::Phase)?;
        let comparisons =
            (usize::BITS - inventory.participants().len().leading_zeros()) as usize + 1;
        context.budget.charge_source_bytes(
            0,
            0,
            checked_product(comparisons, source.relative_path().len())?,
            0,
        )?;
        let participant = inventory
            .participant(source.relative_path())
            .ok_or(Failure::Phase)?;
        let fact = participant.file();
        let bytes = source
            .checked_retained_bytes()
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        // with_persistence_observation also clones native metadata. Its full continuing
        // representation is admitted before the call, even for a later refused Ready bind.
        admission::admit_storage(&mut context.budget, bytes)?;
        let current = match source.with_persistence_observation(
            fact.identity(),
            fact.parent_identity(),
            fact.digest(),
        ) {
            Ok(current) => current,
            Err(error) => {
                context
                    .budget
                    .reconcile_retained_reservation(bytes, 0)
                    .expect("refused observation construction retained no source fact");
                return Err(Failure::Document(error).into());
            }
        };
        sources.push((original.document, current));
        originals.push(index);
    }
    sources.sort_unstable_by_key(|(document, _)| *document);
    originals.sort_unstable_by_key(|index| context.open[*index].document);
    Ok(())
}
