//! Immutable, finite source inputs and unpublished extraction preparation.
//!
//! This owner retains actual source allocations and resolver associations. It does not perform
//! IO, prove an all-profile inventory complete, bind live history, or authorize publication.
//! Retained payload interpretation is conservative: complete physical and modified-context
//! dependencies bound preservation of unrelated unavailable records. Understood fragments
//! still pass through the shared rewrite owner.

use std::{
    collections::{BTreeMap, HashMap},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{AssetRecord, AssetRef, AssetReferenceResolver};

use crate::{
    AuthoringSchemaBasis, ExpandedSceneDocument, PrefabDocument, ResolvedPrefabSource,
    SceneDocument, SceneEntityId, SourceDocumentKey, SourceDocumentKind,
    prefab::{ResolvedPrefabDocument, SceneCompilationContext, SchemaAuthority},
};

use super::{
    ExtractionError, ExtractionWork, SourceCut, SourceDocumentInput,
    construction::{SplitSourceDocument, construct_source_split, verify_source_split},
    correspondence::{OccurrenceCorrespondence, verify_occurrences},
    dependants::{rewrite_dependant, verify_dependant},
    footprint::{SourceContextFootprint, classify_contexts},
    ownership::{MappedRecord, OutputSource, PhysicalDeclarationMap, map_declarations},
    preservation::verify_unavailable_records,
    relocation::BranchRelocation,
};

mod environment;
mod saved_generation;
pub(crate) use saved_generation::{SavedGenerationRefusal, VerifiedExtractionSavedGeneration};
#[cfg(test)]
mod tests;
use environment::EnvironmentLookup;
pub use environment::{
    PrefabExtractionAssets, PrefabExtractionBinding, PrefabExtractionEnvironment,
};

/// Inputs belong to the actual capture owner. A raw allocation with an unrelated nominal
/// basis cannot stand in for a captured source; no Session or history is manufactured here.
#[derive(Debug)]
pub enum CapturedExtractionSource {
    Scene {
        key: SourceDocumentKey,
        document: Arc<SceneDocument>,
        basis: AuthoringSchemaBasis,
    },
    Prefab {
        source: ResolvedPrefabSource,
    },
}

impl CapturedExtractionSource {
    pub fn key(&self) -> &SourceDocumentKey {
        match self {
            Self::Scene { key, .. } => key,
            Self::Prefab { source, .. } => source.key(),
        }
    }

    pub(super) fn document(&self) -> SourceDocumentInput<'_> {
        match self {
            Self::Scene { document, .. } => SourceDocumentInput::Scene(document),
            Self::Prefab { source, .. } => SourceDocumentInput::Prefab(source.document()),
        }
    }
}

/// The identity record is the exact proposed Q metadata, not an invented stable-key alias.
/// Its source owner separately proves that both file destinations were observed absent.
pub(crate) struct ExtractionDestination {
    pub(crate) key: SourceDocumentKey,
    pub(crate) reference: AssetRef,
    pub(crate) metadata: AssetRecord,
}

pub(crate) struct ExtractionRequest {
    pub(crate) sources: Vec<CapturedExtractionSource>,
    pub(crate) environments: Vec<PrefabExtractionEnvironment>,
    pub(crate) selected_environment: usize,
    pub(crate) basis: AuthoringSchemaBasis,
    pub(crate) selected: SourceDocumentKey,
    pub(crate) root: SceneEntityId,
    pub(crate) destination: ExtractionDestination,
    pub(crate) max_depth: usize,
}

/// A refusal transfers the exact request back without recapture or source-tree cloning.
/// Work performed by failed compilers and constructors remains in the returned allowance.
pub(crate) struct ExtractionPreparationFailure {
    pub(crate) request: ExtractionRequest,
    pub(crate) work: ExtractionWork,
    pub(crate) error: ExtractionError,
    pub(crate) environment: Option<usize>,
}

/// Stored inputs and outputs survive a refused pure verification without reconstruction.
pub(crate) struct ExtractionStoredAdmissionFailure {
    pub(crate) request: ExtractionRequest,
    pub(crate) sources: Vec<CapturedExtractionSource>,
    pub(crate) extracted: ResolvedPrefabSource,
    pub(crate) work: ExtractionWork,
    pub(crate) error: ExtractionError,
    pub(crate) environment: Option<usize>,
}

pub(super) struct SourceOccurrenceCorrespondence {
    pub(super) environment: usize,
    pub(super) source: usize,
    pub(super) entities: Vec<OccurrenceCorrespondence>,
}

/// The private preparation owns its originals, outputs and snapshot-relative correspondence.
/// Borrowing outputs grants no mutation, live-source installation, or Native proof authority.
pub(crate) struct PreparedExtractionCorpus {
    corpus: VerifiedExtractionCorpus,
    work: ExtractionWork,
}

/// Immutable verified construction, separate from the enclosing operation's cumulative Work.
/// This data alone grants neither live binding nor publication authority.
pub(crate) struct VerifiedExtractionCorpus {
    original: ExtractionRequest,
    sources: Vec<CapturedExtractionSource>,
    extracted: ResolvedPrefabSource,
    declarations: PhysicalDeclarationMap,
    occurrences: Vec<SourceOccurrenceCorrespondence>,
    changed: Vec<bool>,
    cut: Arc<super::SourceCutMembership>,
}

impl PreparedExtractionCorpus {
    pub(crate) fn into_parts(self) -> (VerifiedExtractionCorpus, ExtractionWork) {
        (self.corpus, self.work)
    }

    pub(crate) fn original(&self) -> &ExtractionRequest {
        self.corpus.original()
    }
    pub(crate) fn sources(&self) -> &[CapturedExtractionSource] {
        self.corpus.sources()
    }
    pub(crate) fn extracted(&self) -> &ResolvedPrefabSource {
        self.corpus.extracted()
    }
    pub(super) fn occurrences(&self) -> &[SourceOccurrenceCorrespondence] {
        self.corpus.occurrences()
    }
    pub(crate) fn usage(&self) -> (usize, usize) {
        self.work.usage()
    }
    pub(super) fn record(&self, source: usize, record: usize) -> Option<MappedRecord<'_>> {
        self.corpus.record(source, record)
    }
    pub(super) fn placement_destination(
        &self,
        source: usize,
        index: usize,
    ) -> Option<(OutputSource, usize)> {
        self.corpus.placement_destination(source, index)
    }
    pub(super) fn generated_placements(&self) -> &[usize] {
        self.corpus.generated_placements()
    }
}

impl VerifiedExtractionCorpus {
    pub(crate) fn source_changed(&self, source: usize) -> Option<bool> {
        self.changed.get(source).copied()
    }
    pub(crate) fn original(&self) -> &ExtractionRequest {
        &self.original
    }

    pub(crate) fn sources(&self) -> &[CapturedExtractionSource] {
        &self.sources
    }

    pub(crate) fn extracted(&self) -> &ResolvedPrefabSource {
        &self.extracted
    }

    pub(super) fn occurrences(&self) -> &[SourceOccurrenceCorrespondence] {
        &self.occurrences
    }

    /// Borrows the actual old/new declaration owner, including every stored nested suffix.
    pub(super) fn record(&self, source: usize, record: usize) -> Option<MappedRecord<'_>> {
        let original = self.original.sources.get(source)?;
        let entities = match original.document() {
            SourceDocumentInput::Scene(document) => &document.entities,
            SourceDocumentInput::Prefab(document) => &document.entities,
        };
        let before = entities.get(record)?;
        let fact = self.declarations.record(source, record)?;
        let (key, entities) = match fact.source {
            OutputSource::Captured(source) => {
                let source = self.sources.get(source)?;
                let entities = match source.document() {
                    SourceDocumentInput::Scene(document) => &document.entities,
                    SourceDocumentInput::Prefab(document) => &document.entities,
                };
                (source.key(), entities)
            }
            OutputSource::Extracted => (self.extracted.key(), &self.extracted.document().entities),
        };
        Some(MappedRecord {
            source: key,
            before,
            after: entities.get(fact.record)?,
        })
    }

    pub(super) fn placement_destination(
        &self,
        source: usize,
        index: usize,
    ) -> Option<(OutputSource, usize)> {
        self.declarations.placement(source, index)
    }

    pub(super) fn generated_placements(&self) -> &[usize] {
        self.declarations.generated_placements()
    }
}

impl ExtractionRequest {
    /// Admits fixed stored outputs through the original semantic owners. This never calls the
    /// complete candidate constructors and never assigns new Q inputs or entity identities.
    pub(crate) fn admit_outputs(
        self,
        sources: Vec<CapturedExtractionSource>,
        extracted: ResolvedPrefabSource,
        mut work: ExtractionWork,
    ) -> Result<PreparedExtractionCorpus, ExtractionStoredAdmissionFailure> {
        let mut environment = None;
        match verify_stored(&self, &sources, &extracted, &mut work, &mut environment) {
            Ok(relations) => Ok(PreparedExtractionCorpus {
                corpus: VerifiedExtractionCorpus {
                    original: self,
                    sources,
                    extracted,
                    declarations: relations.declarations,
                    occurrences: relations.occurrences,
                    changed: relations.changed,
                    cut: relations.cut,
                },
                work,
            }),
            Err(error) => Err(ExtractionStoredAdmissionFailure {
                request: self,
                sources,
                extracted,
                work,
                error,
                environment,
            }),
        }
    }

    pub(crate) fn prepare(
        self,
        mut work: ExtractionWork,
    ) -> Result<PreparedExtractionCorpus, ExtractionPreparationFailure> {
        let mut environment = None;
        match prepare(&self, &mut work, &mut environment) {
            Ok(prepared) => Ok(PreparedExtractionCorpus {
                corpus: VerifiedExtractionCorpus {
                    original: self,
                    sources: prepared.sources,
                    extracted: prepared.extracted,
                    declarations: prepared.declarations,
                    occurrences: prepared.occurrences,
                    changed: prepared.changed,
                    cut: prepared.cut,
                },
                work,
            }),
            Err(error) => Err(ExtractionPreparationFailure {
                request: self,
                work,
                error,
                environment,
            }),
        }
    }
}

struct PreparedOutputs {
    sources: Vec<CapturedExtractionSource>,
    extracted: ResolvedPrefabSource,
    declarations: PhysicalDeclarationMap,
    occurrences: Vec<SourceOccurrenceCorrespondence>,
    changed: Vec<bool>,
    cut: Arc<super::SourceCutMembership>,
}

struct VerifiedSourceRelations {
    declarations: PhysicalDeclarationMap,
    occurrences: Vec<SourceOccurrenceCorrespondence>,
    changed: Vec<bool>,
    cut: Arc<super::SourceCutMembership>,
}

struct CorpusLookup<'a> {
    keys: BTreeMap<&'a SourceDocumentKey, usize>,
    environments: Vec<EnvironmentLookup<'a>>,
    maximum_key_bytes: usize,
}

impl<'a> CorpusLookup<'a> {
    fn new(
        request: &'a ExtractionRequest,
        work: &mut ExtractionWork,
        active_environment: &mut Option<usize>,
    ) -> Result<Self, ExtractionError> {
        work.admit(request.sources.len().checked_mul(256))?;
        let mut keys = BTreeMap::new();
        let mut source_assets = HashMap::new();
        let levels = ((usize::BITS - request.sources.len().leading_zeros()) as usize + 1) * 16;
        // Charge complete key comparisons before inserting borrowed keys into the index.
        work.visit(request.sources.len().saturating_mul(levels), 0, 0)?;
        let mut key_bytes = 0usize;
        let mut maximum_key_bytes = 0usize;
        for source in &request.sources {
            maximum_key_bytes = maximum_key_bytes.max(
                source
                    .key()
                    .checked_retained_bytes()
                    .ok_or(ExtractionError::Capacity)?,
            );
            key_bytes = key_bytes
                .checked_add(
                    source
                        .key()
                        .checked_retained_bytes()
                        .ok_or(ExtractionError::Capacity)?,
                )
                .ok_or(ExtractionError::Capacity)?;
        }
        work.visit(
            0,
            key_bytes
                .checked_mul(levels)
                .ok_or(ExtractionError::Capacity)?,
            0,
        )?;
        for (index, source) in request.sources.iter().enumerate() {
            if keys.insert(source.key(), index).is_some() {
                return Err(ExtractionError::InvalidSource);
            }
            match source {
                CapturedExtractionSource::Scene { key, basis, .. } => {
                    if key.kind() != SourceDocumentKind::Scene || *basis != request.basis {
                        return Err(ExtractionError::InvalidSource);
                    }
                }
                CapturedExtractionSource::Prefab { source } => {
                    if source.key().kind() != SourceDocumentKind::Prefab
                        || !source.matches_authority(SchemaAuthority::Authoring(&request.basis))
                    {
                        return Err(ExtractionError::InvalidSource);
                    }
                    if let Some(asset) = source.source_asset
                        && source_assets.insert(asset, index).is_some()
                    {
                        return Err(ExtractionError::InvalidSource);
                    }
                }
            }
        }
        let destination = &request.destination;
        work.visit(
            levels,
            destination
                .key
                .checked_retained_bytes()
                .and_then(|bytes| bytes.checked_mul(levels))
                .ok_or(ExtractionError::Capacity)?,
            0,
        )?;
        work.visit(1, reference_bytes(&destination.reference)?, 0)?;
        if destination.key.kind() != SourceDocumentKind::Prefab
            || !destination.reference.is_primary()
            || matches!(&destination.reference, AssetRef::StableId(id) if *id != destination.metadata.stable_id())
            || destination
                .reference
                .as_product()
                .is_some_and(|reference| reference.source() != destination.metadata.stable_id())
            || destination
                .metadata
                .validate_source_document_key(&destination.key)
                .is_err()
            || keys.contains_key(&destination.key)
        {
            return Err(ExtractionError::InvalidSource);
        }
        for source in &request.sources {
            work.visit(1, 0, 0)?;
            if matches!(source, CapturedExtractionSource::Prefab { source, .. }
                if source.source_asset == Some(destination.metadata.stable_id()))
            {
                return Err(ExtractionError::InvalidSource);
            }
        }
        let mut lookup = Self {
            keys,
            environments: Vec::new(),
            maximum_key_bytes,
        };
        let selected = lookup
            .key_index(&request.selected, work)?
            .ok_or(ExtractionError::InvalidSource)?;
        if request
            .environments
            .get(request.selected_environment)
            .is_none()
            || request.selected.kind() != SourceDocumentKind::Prefab
        {
            return Err(ExtractionError::InvalidSource);
        }
        work.admit(
            request
                .environments
                .len()
                .checked_mul(size_of::<EnvironmentLookup<'a>>() + 128)
                .and_then(|bytes| {
                    bytes.checked_add(request.sources.len().checked_mul(size_of::<bool>())?)
                }),
        )?;
        let mut names = HashMap::with_capacity(request.environments.len());
        let mut covered = vec![false; request.sources.len()];
        lookup.environments = Vec::with_capacity(request.environments.len());
        for (index, environment) in request.environments.iter().enumerate() {
            *active_environment = Some(index);
            work.visit(1, environment.name.len(), 0)?;
            if names.insert(environment.name.as_str(), index).is_some() {
                return Err(ExtractionError::InvalidSource);
            }
            let resolved = EnvironmentLookup::new(request, &lookup, environment, selected, work)?;
            work.visit(environment.sources.len(), 0, 0)?;
            for member in &environment.sources {
                covered[*member] = true;
            }
            lookup.environments.push(resolved);
        }
        *active_environment = None;
        work.visit(covered.len(), 0, 0)?;
        if covered.iter().any(|covered| !covered) {
            return Err(ExtractionError::InvalidSource);
        }
        *active_environment = Some(request.selected_environment);
        let members = &request.environments[request.selected_environment].sources;
        work.visit(
            (usize::BITS - members.len().leading_zeros()) as usize + 1,
            0,
            0,
        )?;
        if members.binary_search(&selected).is_err() {
            return Err(ExtractionError::InvalidSource);
        }
        *active_environment = None;
        Ok(lookup)
    }

    fn source_index(
        &self,
        environment: usize,
        reference: &AssetRef,
        work: &mut ExtractionWork,
    ) -> Result<Option<usize>, ExtractionError> {
        self.environments
            .get(environment)
            .ok_or(ExtractionError::InvalidSource)?
            .source_index(reference, work)
    }

    fn key_index(
        &self,
        key: &SourceDocumentKey,
        work: &mut ExtractionWork,
    ) -> Result<Option<usize>, ExtractionError> {
        let comparisons = ((usize::BITS - self.keys.len().leading_zeros()) as usize + 1) * 16;
        work.visit(
            comparisons,
            key.checked_retained_bytes()
                .and_then(|bytes| {
                    bytes
                        .checked_add(self.maximum_key_bytes)?
                        .checked_mul(comparisons)
                })
                .ok_or(ExtractionError::Capacity)?,
            0,
        )?;
        Ok(self.keys.get(key).copied())
    }

    fn resolve(
        &self,
        environment: usize,
        request: &AssetRef,
        sources: &[CapturedExtractionSource],
        extra: Option<&ResolvedPrefabSource>,
        work: &mut ExtractionWork,
    ) -> Result<Option<ResolvedPrefabSource>, ExtractionError> {
        self.environments
            .get(environment)
            .ok_or(ExtractionError::InvalidSource)?
            .resolve(request, sources, extra, work)
    }
}

fn prepare(
    request: &ExtractionRequest,
    work: &mut ExtractionWork,
    active_environment: &mut Option<usize>,
) -> Result<PreparedOutputs, ExtractionError> {
    let lookup = CorpusLookup::new(request, work, active_environment)?;
    work.visit(
        lookup.keys.len(),
        request
            .selected
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_mul(lookup.keys.len()))
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    let selected = *lookup
        .keys
        .get(&request.selected)
        .ok_or(ExtractionError::InvalidSource)?;
    let (footprint, footprint_environments) =
        source_contexts(request, &lookup, selected, work, active_environment)?;
    *active_environment = Some(request.selected_environment);
    let cut = capture_cut(request, selected, &lookup, work)?;
    let destination = &request.destination;
    let split = construct_source_split(
        &cut,
        &request.selected,
        &destination.key,
        &destination.reference,
        &request.basis,
        work,
    )?;
    let relocation = BranchRelocation::new(&cut, &request.selected, &destination.key)
        .map_err(|_| ExtractionError::InvalidSource)?;
    work.admit(
        request
            .sources
            .len()
            .checked_mul(size_of::<CapturedExtractionSource>())
            .and_then(|bytes| {
                bytes.checked_add(request.sources.len().checked_mul(size_of::<bool>())?)
            })
            .and_then(|bytes| bytes.checked_add(size_of::<PreparedExtractionCorpus>()))
            .and_then(|bytes| bytes.checked_add(size_of::<PreparedOutputs>())),
    )?;
    let mut sources = Vec::with_capacity(request.sources.len());
    let mut changed = Vec::with_capacity(request.sources.len());
    let mut containing = Some(split.containing);
    for (index, original) in request.sources.iter().enumerate() {
        *active_environment = if index == selected {
            Some(request.selected_environment)
        } else {
            footprint_environments[index]
        };
        let document = if index == selected {
            changed.push(true);
            containing.take().ok_or(ExtractionError::InvalidSource)?
        } else {
            let preserved_context = footprint[index] == SourceContextFootprint::Preserved;
            let rewritten = rewrite_dependant(
                original.document(),
                original.key(),
                &relocation,
                &request.basis,
                preserved_context,
                work,
            )?;
            // Physical context independence permits preserving unavailable fragments. It
            // does not exempt understood dormant references from their canonical rewrite.
            changed.push(rewritten.source_changed);
            if !rewritten.source_changed {
                drop(rewritten);
                sources.push(retain_source(original, work)?);
                continue;
            }
            rewritten.document
        };
        sources.push(replace_document(original, document, &request.basis, work)?);
    }
    *active_environment = None;
    work.admit(
        destination
            .key
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(size_of::<PrefabDocument>()))
            .and_then(|bytes| bytes.checked_add(size_of::<ResolvedPrefabSource>())),
    )?;
    let extracted = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(split.extracted),
        request.basis.clone(),
        destination.key.clone(),
    )
    .with_source_asset(&destination.metadata)
    .map_err(|_| ExtractionError::InvalidSource)?;
    if !extracted.matches_request(&destination.reference) {
        return Err(ExtractionError::InvalidSource);
    }
    let declarations = map_declarations(
        &request.sources,
        &sources,
        selected,
        extracted.document(),
        &split.records,
        &split.placements,
        split.generated_placements,
        &request.root,
        work,
    )?;
    let occurrences = verify_source_occurrences(
        request,
        &sources,
        &extracted,
        &lookup,
        &cut,
        &footprint,
        &declarations,
        work,
        active_environment,
    )?;
    *active_environment = None;
    Ok(PreparedOutputs {
        sources,
        extracted,
        declarations,
        occurrences,
        changed,
        cut: cut.shared_membership(),
    })
}

fn source_contexts(
    request: &ExtractionRequest,
    lookup: &CorpusLookup<'_>,
    selected: usize,
    work: &mut ExtractionWork,
    active_environment: &mut Option<usize>,
) -> Result<(Vec<SourceContextFootprint>, Vec<Option<usize>>), ExtractionError> {
    work.admit(
        request
            .sources
            .len()
            .checked_mul(size_of::<SourceContextFootprint>() + size_of::<Option<usize>>()),
    )?;
    let mut footprint = vec![SourceContextFootprint::Preserved; request.sources.len()];
    let mut causes = vec![None; request.sources.len()];
    for (index, environment) in request.environments.iter().enumerate() {
        *active_environment = Some(index);
        let states = classify_contexts(
            &request.sources,
            &environment.sources,
            selected,
            work,
            |reference, work| lookup.source_index(index, reference, work),
        )?;
        work.visit(states.len(), 0, 0)?;
        for ((combined, cause), state) in footprint.iter_mut().zip(&mut causes).zip(states) {
            let previous = *combined;
            combined.include(state);
            if *combined != previous {
                *cause = Some(index);
            }
        }
    }
    Ok((footprint, causes))
}

fn verify_stored(
    request: &ExtractionRequest,
    sources: &[CapturedExtractionSource],
    extracted: &ResolvedPrefabSource,
    work: &mut ExtractionWork,
    active_environment: &mut Option<usize>,
) -> Result<VerifiedSourceRelations, ExtractionError> {
    let lookup = CorpusLookup::new(request, work, active_environment)?;
    if request.sources.len() != sources.len() {
        return Err(ExtractionError::InvalidSource);
    }
    let authority = SchemaAuthority::Authoring(&request.basis);
    for (before, after) in request.sources.iter().zip(sources) {
        work.visit(
            1,
            before
                .key()
                .checked_retained_bytes()
                .and_then(|bytes| bytes.checked_add(after.key().checked_retained_bytes()?))
                .ok_or(ExtractionError::Capacity)?,
            0,
        )?;
        if before.key() != after.key() {
            return Err(ExtractionError::InvalidSource);
        }
        match (before, after) {
            (
                CapturedExtractionSource::Scene { .. },
                CapturedExtractionSource::Scene { basis, .. },
            ) if basis == &request.basis => {}
            (
                CapturedExtractionSource::Prefab { source: old },
                CapturedExtractionSource::Prefab { source: new },
            ) if new.matches_authority(authority) && new.source_asset == old.source_asset => {}
            _ => return Err(ExtractionError::InvalidSource),
        }
    }
    work.visit(
        1,
        extracted
            .key()
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(request.destination.key.checked_retained_bytes()?))
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    if extracted.key() != &request.destination.key
        || !extracted.matches_authority(authority)
        || extracted.source_asset != Some(request.destination.metadata.stable_id())
        || !extracted.matches_request(&request.destination.reference)
    {
        return Err(ExtractionError::InvalidSource);
    }
    let selected = lookup
        .key_index(&request.selected, work)?
        .ok_or(ExtractionError::InvalidSource)?;
    let (footprint, causes) =
        source_contexts(request, &lookup, selected, work, active_environment)?;
    *active_environment = Some(request.selected_environment);
    let cut = capture_cut(request, selected, &lookup, work)?;
    let destination = &request.destination;
    let split = verify_source_split(
        &cut,
        &request.selected,
        &destination.key,
        &destination.reference,
        &request.basis,
        sources[selected].document(),
        extracted.document(),
        work,
    )?;
    let relocation = BranchRelocation::new(&cut, &request.selected, &destination.key)
        .map_err(|_| ExtractionError::InvalidSource)?;
    work.admit(
        sources
            .len()
            .checked_mul(size_of::<bool>())
            .and_then(|bytes| bytes.checked_add(size_of::<VerifiedSourceRelations>())),
    )?;
    let mut changed = Vec::with_capacity(sources.len());
    for (index, (before, after)) in request.sources.iter().zip(sources).enumerate() {
        if index == selected {
            changed.push(true);
            continue;
        }
        *active_environment = causes[index];
        changed.push(verify_dependant(
            before.document(),
            before.key(),
            after.document(),
            &relocation,
            &request.basis,
            footprint[index] == SourceContextFootprint::Preserved,
            work,
        )?);
    }
    let declarations = map_declarations(
        &request.sources,
        sources,
        selected,
        extracted.document(),
        &split.records,
        &split.placements,
        split.generated_placements,
        &request.root,
        work,
    )?;
    let occurrences = verify_source_occurrences(
        request,
        sources,
        extracted,
        &lookup,
        &cut,
        &footprint,
        &declarations,
        work,
        active_environment,
    )?;
    *active_environment = None;
    Ok(VerifiedSourceRelations {
        declarations,
        occurrences,
        changed,
        cut: cut.shared_membership(),
    })
}

fn verify_source_occurrences(
    request: &ExtractionRequest,
    sources: &[CapturedExtractionSource],
    extracted: &ResolvedPrefabSource,
    lookup: &CorpusLookup<'_>,
    cut: &SourceCut<'_>,
    footprint: &[SourceContextFootprint],
    declarations: &PhysicalDeclarationMap,
    work: &mut ExtractionWork,
    active_environment: &mut Option<usize>,
) -> Result<Vec<SourceOccurrenceCorrespondence>, ExtractionError> {
    let observations = request
        .environments
        .iter()
        .try_fold(0_usize, |count, environment| {
            count.checked_add(environment.sources.len())
        })
        .ok_or(ExtractionError::Capacity)?;
    work.admit(observations.checked_mul(size_of::<SourceOccurrenceCorrespondence>()))?;
    let mut occurrences = Vec::with_capacity(observations);
    for (environment_index, environment) in request.environments.iter().enumerate() {
        *active_environment = Some(environment_index);
        for &index in &environment.sources {
            work.visit(1, 0, 0)?;
            let original = &request.sources[index];
            let SourceDocumentInput::Scene(before) = original.document() else {
                continue;
            };
            let SourceDocumentInput::Scene(after) = sources[index].document() else {
                return Err(ExtractionError::InvalidSource);
            };
            let before = compile_scene(
                before,
                request,
                &request.sources,
                lookup,
                environment_index,
                None,
                work,
                environment
                    .assets_before
                    .as_ref()
                    .map(PrefabExtractionAssets::resolver),
            )?;
            let after = compile_scene(
                after,
                request,
                sources,
                lookup,
                environment_index,
                Some(extracted),
                work,
                environment
                    .assets_after
                    .as_ref()
                    .map(PrefabExtractionAssets::resolver),
            )?;
            let entities = verify_occurrences(
                cut,
                &request.selected,
                &request.destination.key,
                &request.destination.reference,
                original.key(),
                &before.document,
                &after.document,
                &request.basis,
                work,
            )?;
            verify_unavailable_records(
                &before,
                &after,
                &entities,
                &request.sources,
                sources,
                footprint,
                declarations,
                work,
                |source, work| match source {
                    crate::PrefabDeclarationSource::Containing => Ok(index),
                    crate::PrefabDeclarationSource::Prefab(key) => lookup
                        .key_index(key, work)?
                        .ok_or(ExtractionError::InvalidSource),
                },
            )?;
            occurrences.push(SourceOccurrenceCorrespondence {
                environment: environment_index,
                source: index,
                entities,
            });
        }
    }
    Ok(occurrences)
}

fn retain_source(
    original: &CapturedExtractionSource,
    work: &mut ExtractionWork,
) -> Result<CapturedExtractionSource, ExtractionError> {
    work.admit(
        original
            .key()
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(size_of::<CapturedExtractionSource>())),
    )?;
    Ok(match original {
        CapturedExtractionSource::Scene {
            key,
            document,
            basis,
        } => CapturedExtractionSource::Scene {
            key: key.clone(),
            document: Arc::clone(document),
            basis: basis.clone(),
        },
        CapturedExtractionSource::Prefab { source } => CapturedExtractionSource::Prefab {
            source: source.clone(),
        },
    })
}

fn replace_document(
    original: &CapturedExtractionSource,
    document: SplitSourceDocument,
    basis: &AuthoringSchemaBasis,
    work: &mut ExtractionWork,
) -> Result<CapturedExtractionSource, ExtractionError> {
    work.admit(original.key().checked_retained_bytes().and_then(|bytes| {
        bytes.checked_add(size_of::<SceneDocument>().max(size_of::<PrefabDocument>()))
    }))?;
    match (original, document) {
        (CapturedExtractionSource::Scene { key, .. }, SplitSourceDocument::Scene(document)) => {
            Ok(CapturedExtractionSource::Scene {
                key: key.clone(),
                document: Arc::new(document),
                basis: basis.clone(),
            })
        }
        (CapturedExtractionSource::Prefab { source }, SplitSourceDocument::Prefab(document)) => {
            Ok(CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource {
                    document: ResolvedPrefabDocument::AuthoringSource {
                        document: Arc::new(document),
                        basis: basis.clone(),
                    },
                    key: source.key.clone(),
                    source_asset: source.source_asset,
                },
            })
        }
        _ => Err(ExtractionError::InvalidSource),
    }
}

fn reference_bytes(reference: &AssetRef) -> Result<usize, ExtractionError> {
    crate::provenance::provenance_asset_ref_bytes(reference).ok_or(ExtractionError::Capacity)
}

fn capture_cut<'source>(
    request: &'source ExtractionRequest,
    selected: usize,
    lookup: &CorpusLookup<'_>,
    work: &mut ExtractionWork,
) -> Result<SourceCut<'source>, ExtractionError> {
    let options = work.compiler_options(request.max_depth)?;
    let mut loader = |reference: &AssetRef, suspended: &mut crate::SceneCompilationWorkingSet| {
        work.with_suspended_compilation(suspended, |work| {
            lookup.resolve(
                request.selected_environment,
                reference,
                &request.sources,
                None,
                work,
            )
        })
    };
    let mut context = SceneCompilationContext::new(
        SchemaAuthority::Authoring(&request.basis),
        &mut loader,
        options,
        request.environments[request.selected_environment]
            .assets_before
            .as_ref()
            .map(PrefabExtractionAssets::resolver),
    );
    let source = request.sources[selected].document();
    let result = if context.reserve_scratch_bytes(Some(size_of::<SourceCut<'_>>()), None, None) {
        crate::prefab::overlay::compile::capture_source_cut(&mut context, source, &request.root)
            .and_then(|membership| {
                SourceCut::from_membership(
                    source,
                    membership,
                    context.reference_work,
                    |nodes, bytes, allocated| {
                        context
                            .admit_terminal_work(nodes, bytes, allocated)
                            .map_err(|_| crate::SceneCompilationFailureKind::WorkLimit.into())
                    },
                )
            })
    } else {
        Err(crate::SceneCompilationFailureKind::WorkLimit.into())
    };
    if let Err(failure) = &result {
        context.push_expansion_failure(failure);
    }
    let (bytes, references, error, diagnostics, budget_failed) = (
        context.usage.checked_work_bytes(),
        context.reference_work,
        context.load_error,
        context.diagnostics,
        context.budget_failed,
    );
    work.settle_compilation(bytes, references)?;
    if let Some(error) = error {
        return Err(error);
    }
    if budget_failed {
        return Err(ExtractionError::Capacity);
    }
    match result {
        Ok(cut) => Ok(cut),
        Err(failure) => Err(ExtractionError::Compilation {
            diagnostics,
            failure: Some(failure),
            omissions: Vec::new(),
        }),
    }
}

pub(super) struct CompilationObservation {
    pub(super) document: ExpandedSceneDocument,
    pub(super) omissions: Vec<crate::prefab::AuthoringSceneCompilationOmission>,
    pub(super) unavailable: Vec<crate::prefab::UnavailableSourceDeclaration>,
}

fn compile_scene(
    scene: &SceneDocument,
    request: &ExtractionRequest,
    sources: &[CapturedExtractionSource],
    lookup: &CorpusLookup<'_>,
    environment: usize,
    extra: Option<&ResolvedPrefabSource>,
    work: &mut ExtractionWork,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
) -> Result<CompilationObservation, ExtractionError> {
    let options = work.compiler_options(request.max_depth)?;
    let mut loader = |reference: &AssetRef, suspended: &mut crate::SceneCompilationWorkingSet| {
        work.with_suspended_compilation(suspended, |work| {
            lookup.resolve(environment, reference, sources, extra, work)
        })
    };
    let mut context = SceneCompilationContext::new(
        SchemaAuthority::Authoring(&request.basis),
        &mut loader,
        options,
        asset_resolution,
    );
    context.capture_unavailable_declarations = true;
    let (candidate, failure) = context.compile_source_with_terminal(
        &scene.placements,
        &scene.entities,
        None,
        SceneCompilationContext::finish_authoring_document,
    );
    let (bytes, references, error, diagnostics, budget_failed, omissions, unavailable) = (
        context.usage.checked_work_bytes(),
        context.reference_work,
        context.load_error,
        context.diagnostics,
        context.budget_failed,
        context.omissions,
        context.unavailable_declarations,
    );
    work.settle_compilation(bytes, references)?;
    if let Some(error) = error {
        return Err(error);
    }
    if budget_failed {
        return Err(ExtractionError::Capacity);
    }
    if diagnostics.has_errors() || failure.is_some() {
        return Err(ExtractionError::Compilation {
            diagnostics,
            failure,
            omissions,
        });
    }
    let document = candidate.ok_or(ExtractionError::Compilation {
        diagnostics,
        failure,
        omissions: Vec::new(),
    })?;
    Ok(CompilationObservation {
        document,
        omissions,
        unavailable,
    })
}
