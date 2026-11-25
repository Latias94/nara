//! Borrowed structural cut observations for source-owned extraction preparation.
//!
//! These inputs retain their actual Scene/Prefab kind and complete physical declarations.
//! Successful observation proves membership only, not understood relocation or publication.

use std::{mem::size_of, sync::Arc};

use super::*;
use overlay::compile::SourceCutMembership;

mod construction;
mod corpus;
mod correspondence;
mod declarations;
mod dependants;
mod footprint;
mod occurrences;
mod ownership;
mod payload;
mod placements;
mod preservation;
mod relocation;
mod work;

pub use corpus::{
    CapturedExtractionSource, PrefabExtractionAssets, PrefabExtractionBinding,
    PrefabExtractionEnvironment,
};
pub(crate) use corpus::{ExtractionDestination, ExtractionRequest, VerifiedExtractionCorpus};
pub(crate) use corpus::{SavedGenerationRefusal, VerifiedExtractionSavedGeneration};
pub(crate) use work::{ExtractionError, ExtractionWork};

/// Operation-local physical nesting limit, separate from compiler occurrence depth and raw
/// value/format depth. Every pass must inspect the complete accepted tree or refuse.
const MAX_PHYSICAL_DEPTH: usize = 64;

#[derive(Debug)]
pub(crate) struct SourceCut<'a> {
    source: SourceDocumentInput<'a>,
    membership: Arc<SourceCutMembership>,
    entity_ids: BTreeSet<&'a SceneEntityId>,
    maximum_id_bytes: usize,
    reference_work: (usize, usize),
}

impl<'a> SourceCut<'a> {
    fn from_membership<E>(
        source: SourceDocumentInput<'a>,
        membership: SourceCutMembership,
        reference_work: (usize, usize),
        mut admit: impl FnMut(usize, usize, Option<usize>) -> Result<(), E>,
    ) -> Result<Self, E> {
        admit(
            1,
            0,
            size_of::<SourceCutMembership>().checked_add(2 * size_of::<usize>()),
        )?;
        Self::from_shared_membership(
            source,
            Arc::new(membership),
            (reference_work.0.saturating_add(1), reference_work.1),
            admit,
        )
    }

    fn from_shared_membership<E>(
        source: SourceDocumentInput<'a>,
        membership: Arc<SourceCutMembership>,
        mut reference_work: (usize, usize),
        mut admit: impl FnMut(usize, usize, Option<usize>) -> Result<(), E>,
    ) -> Result<Self, E> {
        let records = match source {
            SourceDocumentInput::Scene(document) => &document.entities,
            SourceDocumentInput::Prefab(document) => &document.entities,
        };
        admit(
            membership.entities.len(),
            0,
            membership
                .entities
                .len()
                .checked_mul(size_of::<&SceneEntityId>() + 6 * size_of::<usize>()),
        )?;
        reference_work.0 = reference_work.0.saturating_add(membership.entities.len());
        let mut entity_ids = BTreeSet::new();
        let mut maximum_id_bytes = 0usize;
        for &index in &membership.entities {
            let entity = &records[index].id;
            let comparisons = ((usize::BITS - entity_ids.len().leading_zeros()) as usize + 1) * 16;
            let bytes = entity
                .as_str()
                .len()
                .saturating_add(maximum_id_bytes)
                .saturating_mul(comparisons);
            admit(comparisons, bytes, Some(0))?;
            reference_work.0 = reference_work.0.saturating_add(comparisons);
            reference_work.1 = reference_work.1.saturating_add(bytes);
            entity_ids.insert(entity);
            maximum_id_bytes = maximum_id_bytes.max(entity.as_str().len());
        }
        Ok(Self {
            source,
            membership,
            entity_ids,
            maximum_id_bytes,
            reference_work,
        })
    }

    fn shared_membership(&self) -> Arc<SourceCutMembership> {
        Arc::clone(&self.membership)
    }

    /// One source-owned membership index serves every structural and occurrence relocation.
    /// It borrows the exact immutable input IDs rather than rescan the cut for each reference.
    fn contains_entity<E>(
        &self,
        entity: &SceneEntityId,
        mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<bool, E> {
        let comparisons = ((usize::BITS - self.entity_ids.len().leading_zeros()) as usize + 1) * 16;
        for _ in 0..comparisons {
            admit(entity.as_str().len().checked_add(self.maximum_id_bytes))?;
        }
        Ok(self.entity_ids.contains(entity))
    }

    fn target_fact(&self, target: &crate::PrefabEntityTarget) -> Option<usize> {
        self.membership.targets.get(target).copied()
    }

    fn target_fact_count(&self) -> usize {
        self.membership.target_fact_count
    }

    pub(crate) fn root(&self) -> &'a SceneEntityRecord {
        &self.source_entities()[self.membership.root]
    }

    /// Complete original records, including dormant nested declarations and hidden defaults.
    /// The result never contains a synthesized record for an inherited entity or addition.
    pub(crate) fn entities(&self) -> impl ExactSizeIterator<Item = &'a SceneEntityRecord> + '_ {
        let records = self.source_entities();
        self.membership
            .entities
            .iter()
            .map(move |&index| &records[index])
    }

    pub(crate) const fn source(&self) -> SourceDocumentInput<'a> {
        self.source
    }

    fn source_entities(&self) -> &'a [SceneEntityRecord] {
        match self.source {
            SourceDocumentInput::Scene(document) => &document.entities,
            SourceDocumentInput::Prefab(document) => &document.entities,
        }
    }
}

pub(crate) struct SourceCutReport<'a> {
    pub(crate) cut: Option<SourceCut<'a>>,
    pub(crate) diagnostics: DiagnosticReport,
    pub(crate) failure: Option<SceneCompilationFailure>,
    pub(crate) work_bytes: Option<usize>,
    pub(crate) reference_work: (usize, usize),
    pub(crate) budget_failed: bool,
}

/// Uses the original structural closure without normalizing component values, resolving their
/// references, running codecs or minting a source-edit/Native proof. The caller retains its exact
/// input and actual loader observations for the later ownership and relocation preparation.
pub(crate) fn try_capture_source_cut_with_loader<'source, F, E>(
    source: SourceDocumentInput<'source>,
    root: &SceneEntityId,
    basis: &crate::AuthoringSchemaBasis,
    options: SceneCompilationOptions,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
    mut loader: F,
) -> Result<SourceCutReport<'source>, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let mut context = SceneCompilationContext::new(
        SchemaAuthority::Authoring(basis),
        &mut loader,
        options,
        asset_resolution,
    );
    let result = if context.reserve_scratch_bytes(
        size_of::<SourceCutReport<'_>>().checked_add(size_of::<SourceCut<'_>>()),
        None,
        None,
    ) {
        overlay::compile::capture_source_cut(&mut context, source, root).and_then(|membership| {
            SourceCut::from_membership(
                source,
                membership,
                context.reference_work,
                |nodes, bytes, allocated| {
                    context
                        .admit_terminal_work(nodes, bytes, allocated)
                        .map_err(|_| SceneCompilationFailureKind::WorkLimit.into())
                },
            )
        })
    } else {
        Err(SceneCompilationFailureKind::WorkLimit.into())
    };
    let (cut, failure) = match result {
        Ok(cut) => (Some(cut), None),
        Err(failure) => {
            context.push_expansion_failure(&failure);
            (None, Some(failure))
        }
    };
    if let Some(error) = context.load_error {
        return Err(error);
    }
    Ok(SourceCutReport {
        cut,
        work_bytes: context.usage.checked_work_bytes(),
        reference_work: context.reference_work,
        diagnostics: context.diagnostics,
        failure,
        budget_failed: context.budget_failed,
    })
}
