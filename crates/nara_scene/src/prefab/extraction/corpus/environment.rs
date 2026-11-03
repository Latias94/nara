//! Captured interpretation environments over one physical source table.
//!
//! The capture Host establishes these associations through its actual catalogue and scopes.
//! This module validates their finite internal consistency; it is not a pathname resolver.

use nara_asset::{AssetPathScope, ProjectAssetDatabase};

use super::*;

/// One actual lookup association in a captured configuration. Source identity stays physical.
#[doc(hidden)]
#[derive(Debug)]
pub struct PrefabExtractionBinding {
    pub reference: AssetRef,
    pub source: SourceDocumentKey,
}

/// Actual immutable asset catalogue and authored alias scope for one compilation environment.
/// Its capture owner admits catalogue retention before transferring this shared observation.
#[doc(hidden)]
#[derive(Debug)]
pub struct PrefabExtractionAssets {
    pub database: Arc<ProjectAssetDatabase>,
    pub scope: AssetPathScope,
}

impl PrefabExtractionAssets {
    pub(super) fn resolver(&self) -> AssetReferenceResolver<'_> {
        self.database.resolver(&self.scope)
    }
}

/// One captured profile's participating physical sources and actual before/after resolution.
///
/// Membership is a sorted unique list of request-local source indices. Existing bindings must
/// remain exact; new bindings can select only the proposed Q. The Host separately proves that
/// Q and its metadata are visible in this environment. Names locate refusals, never sources.
#[doc(hidden)]
#[derive(Debug)]
pub struct PrefabExtractionEnvironment {
    pub name: String,
    pub sources: Vec<usize>,
    pub before: Vec<PrefabExtractionBinding>,
    pub after: Vec<PrefabExtractionBinding>,
    pub assets_before: Option<PrefabExtractionAssets>,
    pub assets_after: Option<PrefabExtractionAssets>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Captured(usize),
    Extracted,
}

pub(super) struct EnvironmentLookup<'a> {
    before: HashMap<&'a AssetRef, Target>,
    after: HashMap<&'a AssetRef, Target>,
}

impl<'a> EnvironmentLookup<'a> {
    pub(super) fn new(
        request: &'a ExtractionRequest,
        physical: &CorpusLookup<'_>,
        environment: &'a PrefabExtractionEnvironment,
        selected: usize,
        work: &mut ExtractionWork,
    ) -> Result<Self, ExtractionError> {
        work.visit(1, environment.name.len(), 0)?;
        work.visit(environment.sources.len().saturating_mul(2), 0, 0)?;
        if environment.name.is_empty()
            || environment.sources.is_empty()
            || environment
                .sources
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || environment
                .sources
                .iter()
                .any(|index| *index >= request.sources.len())
        {
            return Err(ExtractionError::InvalidSource);
        }
        work.admit(
            environment
                .before
                .len()
                .checked_add(environment.after.len())
                .and_then(|count| count.checked_mul(128)),
        )?;
        let mut before = HashMap::with_capacity(environment.before.len());
        let mut after = HashMap::with_capacity(environment.after.len());
        for binding in &environment.before {
            work.visit(1, reference_bytes(&binding.reference)?, 0)?;
            let index = physical
                .key_index(&binding.source, work)?
                .ok_or(ExtractionError::InvalidSource)?;
            require_member(&environment.sources, index, work)?;
            let CapturedExtractionSource::Prefab { source } = &request.sources[index] else {
                return Err(ExtractionError::InvalidSource);
            };
            if !matches_source(&binding.reference, source.source_asset)
                || before
                    .insert(&binding.reference, Target::Captured(index))
                    .is_some()
            {
                return Err(ExtractionError::InvalidSource);
            }
        }
        for binding in &environment.after {
            work.visit(
                2,
                reference_bytes(&binding.reference)?
                    .checked_mul(2)
                    .ok_or(ExtractionError::Capacity)?,
                0,
            )?;
            work.visit(
                1,
                binding
                    .source
                    .checked_retained_bytes()
                    .and_then(|bytes| {
                        bytes.checked_add(request.destination.key.checked_retained_bytes()?)
                    })
                    .ok_or(ExtractionError::Capacity)?,
                0,
            )?;
            let target = if binding.source == request.destination.key {
                if !matches_source(
                    &binding.reference,
                    Some(request.destination.metadata.stable_id()),
                ) {
                    return Err(ExtractionError::InvalidSource);
                }
                Target::Extracted
            } else {
                let index = physical
                    .key_index(&binding.source, work)?
                    .ok_or(ExtractionError::InvalidSource)?;
                require_member(&environment.sources, index, work)?;
                let CapturedExtractionSource::Prefab { source } = &request.sources[index] else {
                    return Err(ExtractionError::InvalidSource);
                };
                if !matches_source(&binding.reference, source.source_asset) {
                    return Err(ExtractionError::InvalidSource);
                }
                Target::Captured(index)
            };
            let previous = before.get(&binding.reference).copied();
            if previous.is_some_and(|previous| previous != target)
                || (previous.is_none() && target != Target::Extracted)
                || after.insert(&binding.reference, target).is_some()
            {
                return Err(ExtractionError::InvalidSource);
            }
        }
        for (reference, target) in &before {
            work.visit(1, reference_bytes(reference)?, 0)?;
            if after.get(reference) != Some(target) {
                return Err(ExtractionError::InvalidSource);
            }
        }
        work.visit(1, reference_bytes(&request.destination.reference)?, 0)?;
        work.visit(
            (usize::BITS - environment.sources.len().leading_zeros()) as usize + 1,
            0,
            0,
        )?;
        // P's actual successor contains this Q declaration even when a Scene does not
        // currently demand it. A participating environment cannot receive an unresolved link.
        if environment.sources.binary_search(&selected).is_ok()
            && after.get(&request.destination.reference) != Some(&Target::Extracted)
        {
            return Err(ExtractionError::InvalidSource);
        }
        Ok(Self { before, after })
    }

    pub(super) fn source_index(
        &self,
        reference: &AssetRef,
        work: &mut ExtractionWork,
    ) -> Result<Option<usize>, ExtractionError> {
        work.visit(1, reference_bytes(reference)?, 0)?;
        Ok(match self.before.get(reference) {
            Some(Target::Captured(index)) => Some(*index),
            _ => None,
        })
    }

    pub(super) fn saved_source_index(
        &self,
        reference: &AssetRef,
        extracted_index: usize,
        work: &mut ExtractionWork,
    ) -> Result<Option<usize>, ExtractionError> {
        work.visit(1, reference_bytes(reference)?, 0)?;
        Ok(match self.after.get(reference) {
            Some(Target::Captured(index)) => Some(*index),
            Some(Target::Extracted) => Some(extracted_index),
            None => None,
        })
    }

    pub(super) fn contains_extracted(
        &self,
        work: &mut ExtractionWork,
    ) -> Result<bool, ExtractionError> {
        work.visit(self.after.len(), 0, 0)?;
        Ok(self
            .after
            .values()
            .any(|target| *target == Target::Extracted))
    }

    pub(super) fn resolve(
        &self,
        reference: &AssetRef,
        sources: &[CapturedExtractionSource],
        extracted: Option<&ResolvedPrefabSource>,
        work: &mut ExtractionWork,
    ) -> Result<Option<ResolvedPrefabSource>, ExtractionError> {
        work.visit(1, reference_bytes(reference)?, 0)?;
        let bindings = if extracted.is_some() {
            &self.after
        } else {
            &self.before
        };
        let source = match bindings.get(reference) {
            Some(Target::Captured(index)) => match sources.get(*index) {
                Some(CapturedExtractionSource::Prefab { source }) => Some(source),
                _ => return Err(ExtractionError::InvalidSource),
            },
            Some(Target::Extracted) => extracted,
            None => None,
        };
        source
            .map(|source| {
                work.admit(
                    source
                        .key()
                        .checked_retained_bytes()
                        .and_then(|bytes| bytes.checked_add(size_of::<ResolvedPrefabSource>())),
                )?;
                Ok(source.clone())
            })
            .transpose()
    }
}

fn matches_source(reference: &AssetRef, source: Option<nara_asset::StableAssetId>) -> bool {
    reference.is_primary()
        && match reference {
            AssetRef::Path(_) => true,
            AssetRef::StableId(id) => source == Some(*id),
            AssetRef::Product(reference) => source == Some(reference.source()),
        }
}

fn require_member(
    members: &[usize],
    source: usize,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.visit(
        (usize::BITS - members.len().leading_zeros()) as usize + 1,
        0,
        0,
    )?;
    members
        .binary_search(&source)
        .map(|_| ())
        .map_err(|_| ExtractionError::InvalidSource)
}
