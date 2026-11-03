//! Exact reviewed outputs combined with captured original disk guards.
//!
//! This observation uses the existing compiler and environment lookup. It neither rewrites a
//! source again nor admits an unpublished working copy as a saved dependency.

use super::*;
mod relocation;
#[cfg(test)]
mod tests;

pub(crate) struct VerifiedExtractionSavedGeneration {
    sources: Vec<CapturedExtractionSource>,
    published: Vec<bool>,
    extracted: ResolvedPrefabSource,
}

pub(crate) struct SavedGenerationRefusal {
    pub(crate) environment: Option<usize>,
    pub(crate) source: Option<usize>,
    pub(crate) error: ExtractionError,
}

impl VerifiedExtractionSavedGeneration {
    pub(crate) fn sources(&self) -> &[CapturedExtractionSource] {
        &self.sources
    }
    pub(crate) fn published(&self) -> &[bool] {
        &self.published
    }
    pub(crate) fn extracted(&self) -> &ResolvedPrefabSource {
        &self.extracted
    }
}

impl VerifiedExtractionCorpus {
    pub(crate) fn prepare_saved_generation(
        &self,
        disk: &[CapturedExtractionSource],
        published: &[bool],
        work: &mut ExtractionWork,
    ) -> Result<VerifiedExtractionSavedGeneration, SavedGenerationRefusal> {
        let (mut environment, mut source) = (None, None);
        prepare(self, disk, published, work, &mut environment, &mut source).map_err(|error| {
            SavedGenerationRefusal {
                environment,
                source,
                error,
            }
        })
    }
}

fn prepare(
    corpus: &VerifiedExtractionCorpus,
    disk: &[CapturedExtractionSource],
    published: &[bool],
    work: &mut ExtractionWork,
    active_environment: &mut Option<usize>,
    active_source: &mut Option<usize>,
) -> Result<VerifiedExtractionSavedGeneration, ExtractionError> {
    let request = corpus.original();
    if disk.len() != request.sources.len() || published.len() != disk.len() {
        return Err(ExtractionError::InvalidSource);
    }
    let lookup = CorpusLookup::new(request, work, active_environment)?;
    work.admit(
        disk.len()
            .checked_mul(size_of::<CapturedExtractionSource>() + size_of::<bool>()),
    )?;
    let mut sources = Vec::with_capacity(disk.len());
    let mut publication = Vec::with_capacity(disk.len());
    for (index, ((original_disk, output), &publish)) in
        disk.iter().zip(corpus.sources()).zip(published).enumerate()
    {
        *active_source = Some(index);
        let original = &request.sources[index];
        let bytes = original
            .key()
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(original_disk.key().checked_retained_bytes()?))
            .ok_or(ExtractionError::Capacity)?;
        work.visit(1, bytes, 0)?;
        if original_disk.key() != original.key()
            || (!publish && corpus.source_changed(index) == Some(true))
        {
            return Err(ExtractionError::InvalidSource);
        }
        match (original_disk, original) {
            (
                CapturedExtractionSource::Scene { basis, .. },
                CapturedExtractionSource::Scene { .. },
            ) if basis == &request.basis => {}
            (
                CapturedExtractionSource::Prefab { source: disk },
                CapturedExtractionSource::Prefab { source: original },
            ) if disk.matches_authority(SchemaAuthority::Authoring(&request.basis))
                && disk.source_asset == original.source_asset => {}
            _ => return Err(ExtractionError::InvalidSource),
        }
        sources.push(retain_source(
            if publish { output } else { original_disk },
            work,
        )?);
        publication.push(publish);
    }
    *active_source = None;
    let selected = lookup
        .key_index(&request.selected, work)?
        .ok_or(ExtractionError::InvalidSource)?;
    work.admit(
        disk.len()
            .checked_mul(size_of::<SourceContextFootprint>() + size_of::<Option<usize>>()),
    )?;
    let mut footprint = vec![SourceContextFootprint::Preserved; disk.len()];
    let mut causes = vec![None; disk.len()];
    // Include Q in the physical dependency scan, with its actual after-environment membership.
    // P -> Q is a real edge, not an unresolved lookup which can be discarded as inactive.
    work.admit(
        sources
            .len()
            .checked_add(1)
            .and_then(|count| count.checked_mul(size_of::<CapturedExtractionSource>())),
    )?;
    let mut with_extracted = Vec::with_capacity(sources.len() + 1);
    for source in &sources {
        with_extracted.push(retain_source(source, work)?);
    }
    let extracted_index = with_extracted.len();
    work.admit(
        corpus
            .extracted()
            .key()
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(size_of::<ResolvedPrefabSource>())),
    )?;
    with_extracted.push(CapturedExtractionSource::Prefab {
        source: corpus.extracted().clone(),
    });
    for (environment, configuration) in request.environments.iter().enumerate() {
        *active_environment = Some(environment);
        let before = classify_contexts(
            disk,
            &configuration.sources,
            selected,
            work,
            |reference, work| lookup.source_index(environment, reference, work),
        )?;
        work.admit(
            configuration
                .sources
                .len()
                .checked_add(1)
                .and_then(|count| count.checked_mul(size_of::<usize>())),
        )?;
        let mut after_members = Vec::with_capacity(configuration.sources.len() + 1);
        after_members.extend_from_slice(&configuration.sources);
        if lookup.environments[environment].contains_extracted(work)? {
            after_members.push(extracted_index);
        }
        let after = classify_contexts(
            &with_extracted,
            &after_members,
            selected,
            work,
            |reference, work| {
                lookup.environments[environment].saved_source_index(
                    reference,
                    extracted_index,
                    work,
                )
            },
        )?;
        work.visit(footprint.len().saturating_mul(2), 0, 0)?;
        for (index, (before, after)) in before
            .into_iter()
            .zip(after)
            .take(footprint.len())
            .enumerate()
        {
            let previous = footprint[index];
            footprint[index].include(before);
            footprint[index].include(after);
            if previous != footprint[index] {
                causes[index] = Some(environment);
            }
        }
    }
    for (index, source) in sources.iter().enumerate() {
        *active_environment = causes[index];
        *active_source = Some(index);
        if footprint[index] != SourceContextFootprint::Preserved
            && work.source_has_unavailable_records(source.document(), &request.basis)?
        {
            return Err(ExtractionError::UnknownSchema);
        }
    }
    let cut = SourceCut::from_shared_membership(
        request.sources[selected].document(),
        Arc::clone(&corpus.cut),
        (0, 0),
        |nodes, bytes, allocated| {
            work.visit(nodes, bytes, allocated.ok_or(ExtractionError::Capacity)?)
        },
    )?;
    let relocation = BranchRelocation::new(&cut, &request.selected, &request.destination.key)
        .map_err(|_| ExtractionError::InvalidSource)?;
    for (index, source) in sources.iter().enumerate() {
        if published[index] {
            continue;
        }
        *active_environment = causes[index];
        *active_source = Some(index);
        if relocation::requires_relocation(
            source.document(),
            source.key(),
            &relocation,
            &request.basis,
            footprint[index] == SourceContextFootprint::Preserved,
            work,
        )? {
            return Err(ExtractionError::SavedGuardRequiresRelocation);
        }
    }
    *active_source = None;
    for (environment, configuration) in request.environments.iter().enumerate() {
        *active_environment = Some(environment);
        for &index in &configuration.sources {
            *active_source = Some(index);
            let SourceDocumentInput::Scene(scene) = sources[index].document() else {
                continue;
            };
            // A published Scene sees only this finite saved generation. Dirty open guards
            // cannot supply a missing entity, argument, source or reference during this pass.
            compile_scene(
                scene,
                request,
                &sources,
                &lookup,
                environment,
                Some(corpus.extracted()),
                work,
                configuration
                    .assets_after
                    .as_ref()
                    .map(PrefabExtractionAssets::resolver),
            )?;
        }
    }
    *active_environment = None;
    *active_source = None;
    work.admit(
        corpus
            .extracted()
            .key()
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(size_of::<VerifiedExtractionSavedGeneration>())),
    )?;
    Ok(VerifiedExtractionSavedGeneration {
        sources,
        published: publication,
        extracted: corpus.extracted().clone(),
    })
}
