//! Scene-owned, source-independent processed JSON and its admission boundary.

mod budget;
mod cook;
mod identity;
mod wire;

#[cfg(test)]
mod tests;

use std::{error::Error, fmt};

use nara_asset::ProjectAssetDatabase;
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::ComponentValueMaterializationLimits;

use crate::{
    ExpandedSceneCandidate, SceneComponentBindings, SceneComponentBindingsWitness, SceneDocument,
    SceneEntityProvenanceMap, SceneFileLimits, SourceDocumentKey, SourceDocumentKind,
    diagnostics::{error, with_public_identifier, with_public_u64},
};

/// Concrete bounds for the processed Scene owner, in addition to ordinary Scene file limits.
///
/// Working bytes are cumulative modeled allocation requests, without refunds for discarded
/// scratch. They are not allocator capacity or resident memory. The shared final Schema gate's
/// declared-asset traversal and aggregate inventory join this ledger. Existing hierarchy,
/// default-validation and local-reference scratch, core shape-preflight scratch and allocations
/// internal to the JSON syntax adapter retain their existing base limits; they do not join it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeSceneLimits {
    base: SceneFileLimits,
    working_bytes: ByteLimit,
    provenance_bytes: ByteLimit,
    provenance_nodes: ItemLimit,
    provenance_depth: DepthLimit,
}

impl Default for RuntimeSceneLimits {
    fn default() -> Self {
        Self {
            base: SceneFileLimits::default(),
            working_bytes: ByteLimit::new(256 * 1024 * 1024).expect("nonzero working limit"),
            provenance_bytes: ByteLimit::new(32 * 1024 * 1024).expect("nonzero provenance limit"),
            provenance_nodes: ItemLimit::new(1_000_000).expect("nonzero provenance node limit"),
            provenance_depth: DepthLimit::new(64).expect("nonzero provenance depth limit"),
        }
    }
}

impl RuntimeSceneLimits {
    #[must_use]
    pub const fn base(self) -> SceneFileLimits {
        self.base
    }

    #[must_use]
    pub const fn working_bytes(self) -> ByteLimit {
        self.working_bytes
    }

    #[must_use]
    pub const fn provenance_bytes(self) -> ByteLimit {
        self.provenance_bytes
    }

    /// Counts the containing key, provenance entries and all typed projection steps.
    #[must_use]
    pub const fn provenance_nodes(self) -> ItemLimit {
        self.provenance_nodes
    }

    /// Maximum number of outer-to-inner typed steps in one projection.
    #[must_use]
    pub const fn provenance_depth(self) -> DepthLimit {
        self.provenance_depth
    }

    #[must_use]
    pub const fn value_limits(self) -> ComponentValueMaterializationLimits {
        self.base.component_materialization_limits()
    }

    #[must_use]
    pub const fn with_base(mut self, limits: SceneFileLimits) -> Self {
        self.base = limits;
        self
    }

    #[must_use]
    pub const fn with_working_bytes(mut self, limit: ByteLimit) -> Self {
        self.working_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_provenance_bytes(mut self, limit: ByteLimit) -> Self {
        self.provenance_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_provenance_nodes(mut self, limit: ItemLimit) -> Self {
        self.provenance_nodes = limit;
        self
    }

    #[must_use]
    pub const fn with_provenance_depth(mut self, limit: DepthLimit) -> Self {
        self.provenance_depth = limit;
        self
    }
}

/// Bounded decoded claims. Only consuming admission can create a usable Runtime candidate.
///
/// This type deliberately implements neither `Clone` nor serde traits.
#[derive(Debug)]
pub struct RuntimeSceneDocumentCandidate {
    source_key: SourceDocumentKey,
    document: SceneDocument,
    provenance: SceneEntityProvenanceMap,
    work: budget::WorkBudget,
}

impl RuntimeSceneDocumentCandidate {
    /// Reads the concrete `runtime_scene`, version 1 JSON representation without running codecs.
    pub fn from_json_bytes(
        bytes: &[u8],
        limits: RuntimeSceneLimits,
    ) -> Result<Self, RuntimeSceneError> {
        wire::decode(bytes, limits)
    }

    /// Admits exact containing identity, current definitions, complete provenance and flat data.
    /// No source resolver, migration or native codec participates in this operation.
    pub fn admit(
        mut self,
        expected_source: &SourceDocumentKey,
        bindings: &SceneComponentBindings,
    ) -> Result<RuntimeSceneCandidate, RuntimeSceneError> {
        require_scene_source(&self.source_key)?;
        if &self.source_key != expected_source {
            return Err(RuntimeSceneError::refused(
                "scene.runtime-source-mismatch",
                "Runtime Scene containing identity does not match the expected source",
            ));
        }
        identity::canonical_ids(
            &self.source_key,
            &self.document,
            &self.provenance,
            true,
            &mut self.work,
        )?;
        validate_values(&self.document, bindings, &mut self.work)?;
        self.work.witness(bindings)?;
        Ok(RuntimeSceneCandidate {
            source_key: self.source_key,
            document: self.document,
            provenance: self.provenance,
            definitions: bindings.witness(),
            limits: self.work.limits(),
        })
    }
}

/// Immutable, move-only processed Scene admitted against the exact selected definitions.
///
/// Storage is shared with the ordinary Scene model; its flat wire and authority are distinct
/// from source files and expanded source candidates. No source receipt is manufactured.
#[derive(Debug)]
pub struct RuntimeSceneCandidate {
    source_key: SourceDocumentKey,
    document: SceneDocument,
    provenance: SceneEntityProvenanceMap,
    definitions: SceneComponentBindingsWitness,
    limits: RuntimeSceneLimits,
}

impl RuntimeSceneCandidate {
    /// Cooks an already complete source/overlay expansion without changing it on failure.
    pub fn cook(
        expanded: &ExpandedSceneCandidate,
        bindings: &SceneComponentBindings,
        database: &ProjectAssetDatabase,
        limits: RuntimeSceneLimits,
    ) -> Result<Self, RuntimeSceneError> {
        cook::cook(expanded, bindings, database, limits)
    }

    #[must_use]
    pub const fn source_key(&self) -> &SourceDocumentKey {
        &self.source_key
    }

    #[must_use]
    pub const fn document(&self) -> &SceneDocument {
        &self.document
    }

    #[must_use]
    pub const fn provenance(&self) -> &SceneEntityProvenanceMap {
        &self.provenance
    }

    #[must_use]
    pub const fn definitions(&self) -> &SceneComponentBindingsWitness {
        &self.definitions
    }

    #[must_use]
    pub const fn limits(&self) -> RuntimeSceneLimits {
        self.limits
    }

    /// Serializes the private flat wire, enforcing the same encoded and structural bounds.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, RuntimeSceneError> {
        wire::encode(self, &mut budget::WorkBudget::new(self.limits))
    }
}

/// Structured safe diagnostics from the processed Scene boundary.
#[derive(Debug)]
pub struct RuntimeSceneError {
    diagnostics: DiagnosticReport,
}

impl RuntimeSceneError {
    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    fn refused(code: &'static str, summary: &'static str) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(error(code, summary));
        Self { diagnostics }
    }

    fn budget(kind: &'static str, observed: usize, maximum: usize) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(with_public_u64(
            with_public_u64(
                with_public_identifier(
                    error(
                        "scene.runtime-budget",
                        "Runtime Scene exceeds its selected limit",
                    ),
                    "budget-kind",
                    kind,
                ),
                "observed",
                u64::try_from(observed).unwrap_or(u64::MAX),
            ),
            "maximum",
            u64::try_from(maximum).unwrap_or(u64::MAX),
        ));
        Self { diagnostics }
    }
}

impl fmt::Display for RuntimeSceneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Runtime Scene refused; inspect diagnostics for the failed contract")
    }
}

impl Error for RuntimeSceneError {}

fn require_scene_source(source: &SourceDocumentKey) -> Result<(), RuntimeSceneError> {
    if source.kind() != SourceDocumentKind::Scene {
        return Err(RuntimeSceneError::refused(
            "scene.runtime-source-kind",
            "Runtime Scene requires a containing Scene source identity",
        ));
    }
    Ok(())
}

fn validate_values(
    document: &SceneDocument,
    bindings: &SceneComponentBindings,
    work: &mut budget::WorkBudget,
) -> Result<(), RuntimeSceneError> {
    let limits = work.limits().value_limits();
    let diagnostics = crate::validation::validate_runtime_values_with_admission(
        document,
        bindings,
        limits,
        |nodes, bytes, allocated| work.admit(nodes, bytes, allocated),
    )?;
    if diagnostics.has_errors() {
        return Err(RuntimeSceneError { diagnostics });
    }
    Ok(())
}
