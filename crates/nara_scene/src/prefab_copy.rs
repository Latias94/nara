//! Bounded copying of an explicitly supplied, complete native Prefab source closure.
//!
//! The caller admits source identities and lookup aliases. This owner interprets only native
//! Prefab declarations and current Schema data; it performs no discovery, IO or execution.

mod budget;
mod rewrite;

#[cfg(test)]
mod tests;

use std::{collections::HashMap, error::Error, fmt};

use nara_asset::{AssetProductRef, AssetRef, ImportedProductId};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{
    AssetReferenceRewriteError, ComponentCodecError, ComponentEntityReferenceRewriteError,
    ComponentPropertyError, DeclaredAssetReferenceError,
};

use crate::{AuthoringSchemaBasis, PrefabDocument, SourceDocumentKey, SourceDocumentKind};

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
    /// Contextual defaults and input declarations retain their source-owned omission semantics.
    /// Unknown, non-current or conflicting Schema records are refused even when a later overlay
    /// would hide them. All source data is borrowed and bounded before copying; failure never
    /// changes this request's originals.
    /// Every copied document crosses ordinary source classification without activation. Success
    /// preserves parameterized and cyclic sources; it does not prove any mount can be expanded.
    /// Actual arguments, canonical routes and surviving targets require compilation of that use.
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
        admit_sources(&sources, basis, &mut budget)?;
        Ok(PreparedPrefabCopy { sources })
    }
}

/// Source-valid copied declarations, ready for the Host's existing native encoding/admission path.
///
/// This result carries no activation context, expanded candidate or runtime admission evidence.
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
    EntityReference {
        source: usize,
        error: Box<ComponentEntityReferenceRewriteError<PrefabCopyError>>,
    },
    AssetRewrite {
        source: usize,
        error: Box<AssetReferenceRewriteError<PrefabCopyError, PrefabCopyError>>,
    },
    /// Source classification is distinct from effective occurrence compilation.
    Source {
        source: usize,
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

fn admit_sources(
    sources: &[PreparedPrefabCopySource],
    basis: &AuthoringSchemaBasis,
    budget: &mut CopyBudget,
) -> Result<(), PrefabCopyError> {
    for (index, source) in sources.iter().enumerate() {
        let limits = budget.source_limits()?;
        basis
            .admit_prefab_copy_source(
                &source.document,
                &source.destination_reference,
                limits,
                |_, bytes| budget.allocate(bytes),
            )
            .map_err(|error| match error {
                crate::authoring_decode::CopyAdmissionError::Admission(error) => error,
                crate::authoring_decode::CopyAdmissionError::Source(error) => {
                    PrefabCopyError::Source {
                        source: index,
                        diagnostics: error.into_diagnostics(),
                    }
                }
            })?;
    }
    Ok(())
}
