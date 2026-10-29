//! Bounded copying of an explicitly supplied, complete native Prefab source closure.
//!
//! The caller admits source identities and lookup aliases. This owner interprets only native
//! Prefab declarations and current Schema data; it performs no discovery, IO or execution.

mod budget;
mod rewrite;

#[cfg(test)]
mod tests;

use std::{collections::HashMap, error::Error, fmt, sync::Arc};

use nara_asset::{AssetProductRef, AssetRef, ImportedProductId};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{
    AssetReferenceRewriteError, ComponentCodecError, ComponentPropertyError,
    DeclaredAssetReferenceError,
};

use crate::{
    AuthoringSchemaBasis, PrefabDocument, PrefabExpansionFailure, PrefabInstance,
    ResolvedPrefabSource, SceneDocument, SceneEntityId, SceneEntityRecord, SourceDocumentKey,
    SourceDocumentKind,
};

use budget::CopyBudget;
pub use budget::PrefabCopyLimits;

/// One Host-admitted source, its complete lookup aliases and an independent destination.
///
/// This is a native Prefab closure member, not a general compound source-copy operation.
/// Product fields may reuse an explicitly supplied enclosing source mapping, retaining their
/// product IDs. A destination without stable source identity cannot represent that substitution.
#[derive(Debug)]
pub struct PrefabCopySource {
    original: SourceDocumentKey,
    destination: SourceDocumentKey,
    original_references: Vec<AssetRef>,
    destination_reference: AssetRef,
    document: PrefabDocument,
}

impl PrefabCopySource {
    #[must_use]
    pub const fn new(
        original: SourceDocumentKey,
        destination: SourceDocumentKey,
        original_references: Vec<AssetRef>,
        destination_reference: AssetRef,
        document: PrefabDocument,
    ) -> Self {
        Self {
            original,
            destination,
            original_references,
            destination_reference,
            document,
        }
    }
}

/// A finite, explicitly supplied closure. Every dependency must have an admitted alias here.
#[derive(Debug)]
pub struct PrefabCopyRequest {
    sources: Vec<PrefabCopySource>,
}

impl PrefabCopyRequest {
    #[must_use]
    pub const fn new(sources: Vec<PrefabCopySource>) -> Self {
        Self { sources }
    }

    /// Copies whole declarations while preserving document-local IDs and local references.
    ///
    /// Defaults are materialized before reference rewriting. Unknown, non-current or conflicting
    /// Schema records are refused even when a later overlay would hide them. All source data is
    /// borrowed and bounded before copying; failure never changes this request's originals.
    /// The final validity gate is one production Schema-only compilation of the entire closure.
    pub fn prepare(
        &self,
        basis: &AuthoringSchemaBasis,
        limits: PrefabCopyLimits,
    ) -> Result<PreparedPrefabCopy, PrefabCopyError> {
        if self.sources.is_empty() {
            return Err(PrefabCopyError::EmptyClosure);
        }
        if self.sources.len() > PrefabCopyLimits::MAX_DOCUMENTS {
            return Err(PrefabCopyError::DocumentLimit);
        }
        let mut budget = CopyBudget::new(limits);
        let mapping = CopyMapping::admit(&self.sources, &mut budget)?;
        // Admit all originals before the first recursive Clone, including dormant declarations.
        for source in &self.sources {
            budget.document(&source.document)?;
        }
        budget.allocate(
            self.sources
                .len()
                .checked_mul(std::mem::size_of::<PreparedPrefabCopySource>()),
        )?;
        let mut sources = Vec::with_capacity(self.sources.len());
        for (index, source) in self.sources.iter().enumerate() {
            let mut document = source.document.clone();
            rewrite::copy_document(&mut document, index, basis, &mapping, &mut budget)?;
            budget.allocate(
                source
                    .original
                    .checked_retained_bytes()
                    .and_then(|bytes| {
                        bytes.checked_add(source.destination.checked_retained_bytes()?)
                    })
                    .and_then(|bytes| {
                        bytes.checked_add(budget::asset_bytes(&source.destination_reference))
                    }),
            )?;
            sources.push(PreparedPrefabCopySource {
                original: source.original.clone(),
                destination: source.destination.clone(),
                destination_reference: source.destination_reference.clone(),
                document,
            });
        }
        validate_closure(&sources, basis, &mut budget)?;
        Ok(PreparedPrefabCopy { sources })
    }
}

/// Copied source declarations, ready for the Host's existing native encoding/admission path.
#[derive(Debug)]
pub struct PreparedPrefabCopy {
    sources: Vec<PreparedPrefabCopySource>,
}

impl PreparedPrefabCopy {
    #[must_use]
    pub fn sources(&self) -> &[PreparedPrefabCopySource] {
        &self.sources
    }

    #[must_use]
    pub fn into_sources(self) -> Vec<PreparedPrefabCopySource> {
        self.sources
    }
}

/// One source-owned result. This is data preparation, not permission to write a destination.
#[derive(Debug)]
pub struct PreparedPrefabCopySource {
    original: SourceDocumentKey,
    destination: SourceDocumentKey,
    destination_reference: AssetRef,
    document: PrefabDocument,
}

impl PreparedPrefabCopySource {
    #[must_use]
    pub const fn original(&self) -> &SourceDocumentKey {
        &self.original
    }

    #[must_use]
    pub const fn destination(&self) -> &SourceDocumentKey {
        &self.destination
    }

    #[must_use]
    pub const fn destination_reference(&self) -> &AssetRef {
        &self.destination_reference
    }

    #[must_use]
    pub const fn document(&self) -> &PrefabDocument {
        &self.document
    }

    #[must_use]
    pub fn into_document(self) -> PrefabDocument {
        self.document
    }
}

/// Schema interpretation is required for every stored record, including inactive overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabCopySchemaError {
    UnknownComponent,
    VersionMismatch,
    OwnerMismatch,
    AdapterUnavailable,
}

/// Errors retain bounded source indices instead of copying arbitrary rejected source payloads.
#[derive(Debug)]
pub enum PrefabCopyError {
    EmptyClosure,
    DocumentLimit,
    InvalidDocumentKind {
        source: usize,
    },
    DuplicateOriginal {
        source: usize,
    },
    DuplicateDestination {
        source: usize,
    },
    DestinationNotIndependent {
        source: usize,
    },
    MissingAliases {
        source: usize,
    },
    DuplicateAlias {
        source: usize,
    },
    DestinationAliasCollision {
        source: usize,
    },
    BudgetExceeded,
    UnmappedAssetReference {
        source: usize,
    },
    ProductSourceIdentityUnavailable {
        source: usize,
    },
    AuxiliaryPrefabSource {
        source: usize,
    },
    UnmappedSourceKey {
        source: usize,
    },
    PersistentReference {
        source: usize,
    },
    ReferenceDefaultAbsent {
        source: usize,
    },
    Schema {
        source: usize,
        reason: PrefabCopySchemaError,
    },
    Value {
        source: usize,
        error: ComponentCodecError,
    },
    Property {
        source: usize,
        error: ComponentPropertyError,
    },
    AssetReference {
        source: usize,
        error: DeclaredAssetReferenceError,
    },
    AssetRewrite {
        source: usize,
        error: Box<AssetReferenceRewriteError<PrefabCopyError, PrefabCopyError>>,
    },
    /// Production compiler failures remain distinct from copying and reference interpretation.
    Compiler {
        failure: Option<PrefabExpansionFailure>,
        diagnostics: DiagnosticReport,
    },
}

impl fmt::Display for PrefabCopyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Prefab closure copy preparation failed: {self:?}"
        )
    }
}

impl Error for PrefabCopyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Value { error, .. } => Some(error),
            Self::Property { error, .. } => Some(error),
            Self::AssetReference { error, .. } => Some(error),
            Self::AssetRewrite { error, .. } => Some(error.as_ref()),
            _ => None,
        }
    }
}

struct CopyMapping<'a> {
    sources: &'a [PrefabCopySource],
    aliases: HashMap<&'a AssetRef, usize>,
}

impl<'a> CopyMapping<'a> {
    fn admit(
        sources: &'a [PrefabCopySource],
        budget: &mut CopyBudget,
    ) -> Result<Self, PrefabCopyError> {
        let mut count = 0usize;
        for (index, source) in sources.iter().enumerate() {
            if source.original.kind() != SourceDocumentKind::Prefab
                || source.destination.kind() != SourceDocumentKind::Prefab
            {
                return Err(PrefabCopyError::InvalidDocumentKind { source: index });
            }
            if source.original_references.is_empty() {
                return Err(PrefabCopyError::MissingAliases { source: index });
            }
            budget.charge(
                1,
                source
                    .original
                    .retained_bytes()
                    .saturating_add(source.destination.retained_bytes()),
                0,
            )?;
            budget.charge(1, budget::asset_bytes(&source.destination_reference), 0)?;
            for (other_index, other) in sources.iter().enumerate() {
                budget.charge(
                    1,
                    source
                        .destination
                        .retained_bytes()
                        .saturating_add(other.original.retained_bytes()),
                    0,
                )?;
                if other.original == source.destination {
                    return Err(PrefabCopyError::DestinationNotIndependent { source: index });
                }
                if other_index < index {
                    budget.charge(
                        2,
                        source
                            .original
                            .retained_bytes()
                            .saturating_add(other.original.retained_bytes())
                            .saturating_add(source.destination.retained_bytes())
                            .saturating_add(other.destination.retained_bytes()),
                        0,
                    )?;
                    if other.original == source.original {
                        return Err(PrefabCopyError::DuplicateOriginal { source: index });
                    }
                    if other.destination == source.destination {
                        return Err(PrefabCopyError::DuplicateDestination { source: index });
                    }
                    budget.charge(
                        1,
                        budget::asset_bytes(&source.destination_reference)
                            .saturating_add(budget::asset_bytes(&other.destination_reference)),
                        0,
                    )?;
                    if other.destination_reference == source.destination_reference {
                        return Err(PrefabCopyError::DestinationAliasCollision { source: index });
                    }
                }
            }
            for reference in &source.original_references {
                // Conservative hash-table slot/scratch model; admit before table allocation.
                budget.charge(1, budget::asset_bytes(reference), 512)?;
                count = count
                    .checked_add(1)
                    .ok_or(PrefabCopyError::BudgetExceeded)?;
            }
        }
        let mut aliases = HashMap::with_capacity(count);
        for (index, source) in sources.iter().enumerate() {
            for reference in &source.original_references {
                if aliases.insert(reference, index).is_some() {
                    return Err(PrefabCopyError::DuplicateAlias { source: index });
                }
            }
        }
        for (index, source) in sources.iter().enumerate() {
            if aliases.contains_key(&source.destination_reference) {
                return Err(PrefabCopyError::DestinationAliasCollision { source: index });
            }
        }
        Ok(Self { sources, aliases })
    }

    fn asset(
        &self,
        reference: &AssetRef,
        source: usize,
        budget: &mut CopyBudget,
    ) -> Result<AssetRef, PrefabCopyError> {
        budget.charge(1, budget::asset_bytes(reference), 0)?;
        // A complete explicitly admitted selection wins over enclosing-source substitution.
        // Source substitution never invents a product mapping or rekeys an inner product ID.
        let exact = self.aliases.get(reference).copied();
        let index = match exact {
            Some(index) => index,
            None => {
                let product = reference
                    .as_product()
                    .ok_or(PrefabCopyError::UnmappedAssetReference { source })?;
                let primary = AssetRef::Product(
                    AssetProductRef::new(product.source(), ImportedProductId::PRIMARY)
                        .expect("an admitted product has a non-nil source"),
                );
                if let Some(index) = self
                    .aliases
                    .get(&AssetRef::StableId(product.source()))
                    .or_else(|| self.aliases.get(&primary))
                {
                    *index
                } else {
                    let mut selected = None;
                    for (index, candidate) in self.sources.iter().enumerate() {
                        budget.charge(1, candidate.original.retained_bytes(), 0)?;
                        if matches!(
                            candidate.original.origin(),
                            crate::SourceDocumentOrigin::StableAsset { stable_id }
                                if *stable_id == product.source()
                        ) {
                            selected = Some(index);
                            break;
                        }
                    }
                    selected.ok_or(PrefabCopyError::UnmappedAssetReference { source })?
                }
            }
        };
        let mapped = &self.sources[index];
        let destination = &mapped.destination_reference;
        budget.charge(
            1,
            budget::asset_bytes(destination),
            budget::asset_bytes(destination),
        )?;
        if let Some(product) = reference.as_product() {
            if exact.is_some() && destination.as_product().is_some() {
                return Ok(destination.clone());
            }
            let stable_id = match destination {
                AssetRef::StableId(id) => Some(*id),
                AssetRef::Product(reference) if destination.is_primary() => {
                    Some(reference.source())
                }
                AssetRef::Path(_) => match mapped.destination.origin() {
                    crate::SourceDocumentOrigin::StableAsset { stable_id } => Some(*stable_id),
                    _ => None,
                },
                AssetRef::Product(_) => None,
            }
            .ok_or(PrefabCopyError::ProductSourceIdentityUnavailable { source })?;
            let rewritten = AssetRef::Product(
                AssetProductRef::new(stable_id, product.product())
                    .map_err(|_| PrefabCopyError::ProductSourceIdentityUnavailable { source })?,
            );
            // Charge the inline replacement separately; Reflect admits its complete nested
            // canonical value before allocating any encoded strings or map nodes.
            budget.charge(1, budget::asset_bytes(&rewritten), 0)?;
            return Ok(rewritten);
        }
        Ok(destination.clone())
    }

    fn key(
        &self,
        key: &mut SourceDocumentKey,
        source: usize,
        budget: &mut CopyBudget,
    ) -> Result<(), PrefabCopyError> {
        for candidate in self.sources {
            budget.charge(
                1,
                key.retained_bytes()
                    .saturating_add(candidate.original.retained_bytes()),
                0,
            )?;
            if candidate.original == *key {
                budget.allocate(candidate.destination.checked_retained_bytes())?;
                *key = candidate.destination.clone();
                return Ok(());
            }
        }
        Err(PrefabCopyError::UnmappedSourceKey { source })
    }
}

fn validate_closure(
    sources: &[PreparedPrefabCopySource],
    basis: &AuthoringSchemaBasis,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    // One compiler request shares all expansion, default and reference limits. Every supplied
    // member is instantiated, including members that no other member happens to reach.
    budget.allocate(sources.len().checked_mul(1024))?;
    let mut resolved = Vec::with_capacity(sources.len());
    let mut roots = Vec::with_capacity(sources.len());
    let mut entities = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        budget.document(&source.document)?;
        budget.allocate(source.destination.checked_retained_bytes())?;
        resolved.push(ResolvedPrefabSource::bound_authoring_source(
            Arc::new(source.document.clone()),
            basis.clone(),
            source.destination.clone(),
        ));
        // These IDs exist only in disposable compiler input; copied local identities never change.
        let id = SceneEntityId::new(format!("prefab-copy-{index}"))
            .map_err(|_| PrefabCopyError::BudgetExceeded)?;
        roots.push(id.clone());
        let mut anchor = SceneEntityRecord::new(id);
        budget.allocate(Some(budget::asset_bytes(&source.destination_reference)))?;
        anchor.prefab = Some(PrefabInstance {
            source: source.destination_reference.clone(),
            overrides: None,
        });
        entities.push(anchor);
    }
    let document = SceneDocument { roots, entities };
    let options = budget.compiler_options()?;
    // The production report exposes allocation cost, not its logical reference-byte counter.
    // Reserve that finite traversal allowance separately so the two cannot each spend the
    // whole remaining request budget. All source preparation above charges actual deltas.
    budget.allocate(Some(options.limits.reference_work_bytes().get()))?;
    let report = crate::prefab::try_expand_authoring_prefabs_with_loader(
        &document,
        basis,
        options,
        None,
        |reference, working| {
            let index = sources
                .iter()
                .position(|source| &source.destination_reference == reference)
                .or_else(|| {
                    let product = reference
                        .as_product()
                        .filter(|product| product.product() == ImportedProductId::PRIMARY)?;
                    // Copying preserves explicit PRIMARY selections even when the preferred
                    // destination spelling is a path or stable ID. Admission makes destination
                    // keys unique; only their actual stable origin can prove this fallback.
                    sources.iter().position(|source| {
                        matches!(
                            source.destination.origin(),
                            crate::SourceDocumentOrigin::StableAsset { stable_id }
                                if *stable_id == product.source()
                        )
                    })
                })
                .ok_or(PrefabCopyError::UnmappedAssetReference {
                    source: sources.len(),
                })?;
            // Arc handles share the admitted document; project-location keys still own a path.
            // Debit that key copy before Clone, then tighten the suspended compiler's allowance.
            budget.allocate(sources[index].destination.checked_retained_bytes())?;
            let limit = budget.compiler_working_bytes()?;
            if working
                .checked_work_bytes()
                .is_none_or(|bytes| bytes > limit.get())
            {
                return Err(PrefabCopyError::BudgetExceeded);
            }
            working.restrict_working_bytes(limit);
            Ok(Some(resolved[index].clone()))
        },
    )?;
    budget.allocate(report.checked_work_bytes())?;
    if report.candidate.is_none()
        || report.diagnostics.has_errors()
        || report.omissions.iter().any(|omission| {
            matches!(
                omission.reason,
                crate::prefab::AuthoringPrefabOmissionReason::RecordUnavailable(_)
            )
        })
    {
        return Err(PrefabCopyError::Compiler {
            failure: report.failure,
            diagnostics: report.diagnostics,
        });
    }
    Ok(())
}
