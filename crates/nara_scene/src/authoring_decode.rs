//! Registry-free Scene and Prefab admission for persistent authoring.

mod structure;

use structure::{authoring_structure_rejection, prepare_scene_authoring_structure};

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    num::NonZeroU64,
    sync::Arc,
};

use nara_asset::AssetRef;
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, CatalogFingerprint, ComponentCapability,
    ComponentFieldId, ComponentFieldPathSegment, ComponentSchema,
    ComponentSchemaContributionReceipt, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueShape, DeclaredAssetReference, EntityReference, PersistentComponentSchemaOwnerId,
    collect_declared_asset_references_from_schema_with_limits, effective_component_value,
};

use crate::{
    PrefabDocument, PrefabDocumentCandidate, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SceneFileLimits, ScenePatchDocument,
    ScenePatchOperation, ScenePatchReport,
    diagnostics::{
        error as diagnostic_error, warning as diagnostic_warning, with_asset_ref, with_codec_error,
        with_public_locator,
    },
    validation::{
        reference_traversal_limits, validate_effective_local_references, validate_scene_structure,
    },
};

/// Stable project-relative or stable-asset identity of one persistent authoring document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthoringDocumentSource(AssetRef);

impl AuthoringDocumentSource {
    #[must_use]
    pub const fn new(source: AssetRef) -> Self {
        Self(source)
    }

    #[must_use]
    pub const fn as_asset_ref(&self) -> &AssetRef {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthoringDocumentKind {
    Scene,
    Prefab,
}

/// Readiness of one structurally admitted authoring document.
///
/// Rejected input never reaches this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthoringDocumentReadiness {
    Complete,
    Degraded,
}

/// Read-only identity of one immutable authoring schema and Adapter admission.
///
/// Clones compare equal while they retain the same admission in this process. Independent
/// admissions compare unequal even when their catalog contents match. This is neither a
/// serialized catalog fingerprint nor authority to mutate a document.
#[derive(Clone)]
pub struct AuthoringSchemaBasis(Arc<AuthoringClassificationBasis>);

/// Owned admission for preparing another document against one session's exact authoring basis.
///
/// This value retains schema classification, source identity, history limits and the unavailable
/// record preservation expectation. It owns no working document, history entries, live instance
/// or permission to replace a Workspace document. A Host must still admit a prepared result to
/// its original document and revision before publishing it.
#[derive(Debug)]
pub struct AuthoringDocumentReadmission {
    source: AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    basis: Arc<AuthoringClassificationBasis>,
    history_limits: crate::SceneAuthoringHistoryLimits,
    expected_readiness: AuthoringDocumentReadiness,
    expected_unavailable_manifest: UnavailableRecordManifest,
}

impl AuthoringDocumentReadmission {
    /// Prepares a new Scene lineage using the captured source and classification basis.
    pub fn reopen_scene_candidate(
        &self,
        candidate: SceneDocumentCandidate,
    ) -> Result<crate::SceneAuthoringSession, AuthoringSessionReopenError> {
        if self.kind != AuthoringDocumentKind::Scene {
            return Err(
                authoring_reopen_kind_rejection(AuthoringDocumentKind::Scene, self.kind).into(),
            );
        }
        let (document, limits) = candidate.into_authoring_parts();
        let state = classify_scene_document(
            &document,
            self.source.clone(),
            AuthoringDocumentKind::Scene,
            Arc::clone(&self.basis),
            limits,
        )?;
        Ok(
            crate::SceneAuthoringSession::from_published_authoring_document_with_history_limits(
                PublishedAuthoringSceneDocument { document, state },
                self.history_limits,
            ),
        )
    }

    /// Prepares a new Prefab lineage using the captured source and classification basis.
    pub fn reopen_prefab_candidate(
        &self,
        candidate: PrefabDocumentCandidate,
    ) -> Result<crate::PrefabAuthoringSession, AuthoringSessionReopenError> {
        if self.kind != AuthoringDocumentKind::Prefab {
            return Err(
                authoring_reopen_kind_rejection(AuthoringDocumentKind::Prefab, self.kind).into(),
            );
        }
        let (document, limits) = candidate.into_authoring_parts();
        let state = classify_scene_document(
            &document.instantiate(),
            self.source.clone(),
            AuthoringDocumentKind::Prefab,
            Arc::clone(&self.basis),
            limits,
        )?;
        Ok(
            crate::PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
                PublishedAuthoringPrefabDocument { document, state },
                self.history_limits,
            ),
        )
    }

    /// Checks that re-admission preserved readiness and every unavailable Scene record.
    #[must_use]
    pub fn preserves_scene(&self, session: &crate::SceneAuthoringSession) -> bool {
        self.kind == AuthoringDocumentKind::Scene
            && session.authoring_schema_basis()
                == Some(AuthoringSchemaBasis(Arc::clone(&self.basis)))
            && session.authoring_readiness() == Some(self.expected_readiness)
            && session.unavailable_record_manifest() == Some(&self.expected_unavailable_manifest)
    }

    /// Checks that re-admission preserved readiness and every unavailable Prefab record.
    #[must_use]
    pub fn preserves_prefab(&self, session: &crate::PrefabAuthoringSession) -> bool {
        self.kind == AuthoringDocumentKind::Prefab
            && session.authoring_schema_basis()
                == Some(AuthoringSchemaBasis(Arc::clone(&self.basis)))
            && session.authoring_readiness() == Some(self.expected_readiness)
            && session.unavailable_record_manifest() == Some(&self.expected_unavailable_manifest)
    }
}

impl fmt::Debug for AuthoringSchemaBasis {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringSchemaBasis")
            .finish_non_exhaustive()
    }
}

impl PartialEq for AuthoringSchemaBasis {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for AuthoringSchemaBasis {}

/// ADR 0090 readiness of one persistent component record or component-targeting operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthoringRecordReadiness {
    Available,
    KnownUnbound,
    UnknownSchema,
    Unmigratable,
    FutureSchema,
}

impl AuthoringRecordReadiness {
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

/// Owner identity actually carried by the persistent record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthoringRecordOwnerClaim {
    Declared(PersistentComponentSchemaOwnerId),
    Unclaimed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthoringPatchRecordSlot {
    AddEntity,
    RestoreEntity,
    RestoreDescendant { index: usize },
    AddComponent,
    ReplaceComponent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthoringRecordLocation {
    EntityComponent {
        entity: SceneEntityId,
    },
    PatchComponent {
        anchor: SceneEntityId,
        operation_path: Vec<usize>,
        slot: AuthoringPatchRecordSlot,
        entity: SceneEntityId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthoringPatchTargetKind {
    RemoveComponent,
    SetField,
    RemoveField,
    SetAssetRefField,
}

/// Stable process-independent identity of one authoring Adapter implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthoringAdapterIdentity {
    id: &'static str,
    version: u32,
}

impl AuthoringAdapterIdentity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.id
    }

    #[must_use]
    pub const fn version(self) -> u32 {
        self.version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthoringAdapterEvidence {
    identity: AuthoringAdapterIdentity,
    adapter_generation: NonZeroU64,
    catalog: AuthoringAdapterCatalog,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AuthoringAdapterCatalog {
    Native(ComponentSchemaContributionReceipt),
    Data {
        owner: PersistentComponentSchemaOwnerId,
        generation: u64,
        fingerprint: CatalogFingerprint,
        predecessor: Option<CatalogFingerprint>,
    },
}

impl AuthoringAdapterCatalog {
    fn matches(&self, owner: &AuthoringSchemaBundleOwner) -> bool {
        match self {
            Self::Native(contribution) => {
                let receipt = contribution.owner();
                owner
                    .owner_id()
                    .matches_executable(receipt.owner())
                    .unwrap_or(false)
                    && owner.current().generation() == receipt.generation()
                    && owner.current().fingerprint() == receipt.catalog()
                    && owner.current().predecessor().copied() == receipt.predecessor()
            }
            Self::Data {
                owner: id,
                generation,
                fingerprint,
                predecessor,
            } => {
                owner.owner_id() == id
                    && owner.current().generation() == *generation
                    && owner.current().fingerprint() == *fingerprint
                    && owner.current().predecessor() == predecessor.as_ref()
            }
        }
    }
}

impl AuthoringAdapterEvidence {
    #[must_use]
    pub const fn identity(&self) -> AuthoringAdapterIdentity {
        self.identity
    }

    #[must_use]
    pub const fn adapter_generation(&self) -> u64 {
        self.adapter_generation.get()
    }

    #[must_use]
    pub const fn contribution(&self) -> Option<ComponentSchemaContributionReceipt> {
        match self.catalog {
            AuthoringAdapterCatalog::Native(contribution) => Some(contribution),
            AuthoringAdapterCatalog::Data { .. } => None,
        }
    }

    #[must_use]
    pub const fn supports_native_preview(&self) -> bool {
        matches!(self.catalog, AuthoringAdapterCatalog::Native(_))
    }
}

/// Stable path to an entity-reference leaf retained from an unavailable value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnavailableValuePathSegment {
    Field(String),
    Index(usize),
}

/// Typed entity reference retained separately from the unavailable record's full value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableEntityReference {
    path: Vec<UnavailableValuePathSegment>,
    reference: EntityReference,
}

impl UnavailableEntityReference {
    #[must_use]
    pub fn path(&self) -> &[UnavailableValuePathSegment] {
        &self.path
    }

    #[must_use]
    pub const fn reference(&self) -> &EntityReference {
        &self.reference
    }
}

/// Immutable classification fact for one complete component record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringRecordFact {
    location: AuthoringRecordLocation,
    owner: AuthoringRecordOwnerClaim,
    resolved_owner: Option<PersistentComponentSchemaOwnerId>,
    component: ComponentTypeId,
    version: ComponentSchemaVersion,
    readiness: AuthoringRecordReadiness,
    adapter: Option<AuthoringAdapterEvidence>,
    schema_declared_asset_references: Vec<DeclaredAssetReference>,
}

pub const MAX_AUTHORING_FIELD_WRITES: usize = 8;
const MAX_AUTHORING_FIELD_VALUE_NODES: usize = 256;
const MAX_AUTHORING_FIELD_LOGICAL_BYTES: usize = 64 * 1024;

/// One exact field write within a preservation-safe authoring edit.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoringFieldWrite {
    field: ComponentFieldId,
    value: ComponentValue,
}

impl AuthoringFieldWrite {
    #[must_use]
    pub const fn new(field: ComponentFieldId, value: ComponentValue) -> Self {
        Self { field, value }
    }

    #[must_use]
    pub const fn field(&self) -> &ComponentFieldId {
        &self.field
    }

    #[must_use]
    pub const fn value(&self) -> &ComponentValue {
        &self.value
    }
}

/// One bounded preservation-safe field edit targeting an Available direct component record.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoringFieldEdit {
    entity: SceneEntityId,
    component: ComponentTypeId,
    component_version: ComponentSchemaVersion,
    writes: Vec<AuthoringFieldWrite>,
}

impl AuthoringFieldEdit {
    #[must_use]
    pub fn single(
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
        value: ComponentValue,
    ) -> Result<Self, AuthoringFieldEditError> {
        Self::new(
            entity,
            component,
            component_version,
            [AuthoringFieldWrite::new(field, value)],
        )
    }

    pub fn new(
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        writes: impl IntoIterator<Item = AuthoringFieldWrite>,
    ) -> Result<Self, AuthoringFieldEditError> {
        let writes = writes
            .into_iter()
            .take(MAX_AUTHORING_FIELD_WRITES + 1)
            .collect::<Vec<_>>();
        if writes.is_empty() {
            return Err(AuthoringFieldEditError::Empty);
        }
        if writes.len() > MAX_AUTHORING_FIELD_WRITES {
            return Err(AuthoringFieldEditError::WriteLimitExceeded);
        }
        let mut fields = BTreeSet::new();
        let mut value_nodes = 0usize;
        let mut logical_bytes = 0usize;
        for write in &writes {
            if !fields.insert(write.field.clone()) {
                return Err(AuthoringFieldEditError::DuplicateField);
            }
            let cost = write.value.cost();
            value_nodes = value_nodes.saturating_add(cost.nodes());
            logical_bytes = logical_bytes.saturating_add(cost.logical_bytes());
        }
        if value_nodes > MAX_AUTHORING_FIELD_VALUE_NODES
            || logical_bytes > MAX_AUTHORING_FIELD_LOGICAL_BYTES
        {
            return Err(AuthoringFieldEditError::ValueBudgetExceeded);
        }
        Ok(Self {
            entity,
            component,
            component_version,
            writes,
        })
    }

    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    #[must_use]
    pub const fn component_version(&self) -> ComponentSchemaVersion {
        self.component_version
    }

    #[must_use]
    pub fn writes(&self) -> &[AuthoringFieldWrite] {
        &self.writes
    }

    /// Lowers this edit into the same field operations used by document transactions.
    #[must_use]
    pub fn to_patch(&self) -> ScenePatchDocument {
        ScenePatchDocument::new(
            self.writes
                .iter()
                .map(|write| ScenePatchOperation::SetField {
                    entity: self.entity.clone(),
                    component: self.component.clone(),
                    component_version: self.component_version,
                    field: write.field.clone(),
                    value: write.value.clone(),
                }),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringFieldEditError {
    Empty,
    WriteLimitExceeded,
    DuplicateField,
    ValueBudgetExceeded,
}

impl Display for AuthoringFieldEditError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "an authoring field edit requires at least one write",
            Self::WriteLimitExceeded => "the authoring field edit write limit was exceeded",
            Self::DuplicateField => "an authoring field edit cannot write one field twice",
            Self::ValueBudgetExceeded => "the authoring field edit value budget was exceeded",
        };
        formatter.write_str(message)
    }
}

impl Error for AuthoringFieldEditError {}

impl AuthoringRecordFact {
    #[must_use]
    pub const fn location(&self) -> &AuthoringRecordLocation {
        &self.location
    }

    #[must_use]
    pub const fn owner(&self) -> &AuthoringRecordOwnerClaim {
        &self.owner
    }

    #[must_use]
    pub const fn resolved_owner(&self) -> Option<&PersistentComponentSchemaOwnerId> {
        self.resolved_owner.as_ref()
    }

    #[must_use]
    pub const fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    #[must_use]
    pub const fn version(&self) -> ComponentSchemaVersion {
        self.version
    }

    #[must_use]
    pub const fn readiness(&self) -> AuthoringRecordReadiness {
        self.readiness
    }

    #[must_use]
    pub const fn adapter(&self) -> Option<&AuthoringAdapterEvidence> {
        self.adapter.as_ref()
    }

    #[must_use]
    pub fn schema_declared_asset_references(&self) -> &[DeclaredAssetReference] {
        &self.schema_declared_asset_references
    }
}

/// Complete semantic value retained for one unavailable component record.
#[derive(Debug, Clone, PartialEq)]
pub struct UnavailableRecord {
    source: AuthoringDocumentSource,
    document_kind: AuthoringDocumentKind,
    location: AuthoringRecordLocation,
    placement: Option<crate::ScenePlacement>,
    anchor_placement: Option<crate::ScenePlacement>,
    owner: AuthoringRecordOwnerClaim,
    component: ComponentTypeId,
    version: ComponentSchemaVersion,
    value: ComponentValue,
    entity_references: Vec<UnavailableEntityReference>,
    schema_declared_asset_references: Vec<DeclaredAssetReference>,
    readiness: AuthoringRecordReadiness,
}

impl UnavailableRecord {
    /// Stable parent/root and relative ordering of the source-local owning entity.
    #[must_use]
    pub const fn placement(&self) -> Option<&crate::ScenePlacement> {
        self.placement.as_ref()
    }

    /// Placement of the source-local entity containing a nested Prefab patch record.
    #[must_use]
    pub const fn anchor_placement(&self) -> Option<&crate::ScenePlacement> {
        self.anchor_placement.as_ref()
    }
    #[must_use]
    pub const fn source(&self) -> &AuthoringDocumentSource {
        &self.source
    }

    #[must_use]
    pub const fn document_kind(&self) -> AuthoringDocumentKind {
        self.document_kind
    }

    #[must_use]
    pub const fn location(&self) -> &AuthoringRecordLocation {
        &self.location
    }

    #[must_use]
    pub const fn owner(&self) -> &AuthoringRecordOwnerClaim {
        &self.owner
    }

    #[must_use]
    pub const fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    #[must_use]
    pub const fn version(&self) -> ComponentSchemaVersion {
        self.version
    }

    #[must_use]
    pub const fn value(&self) -> &ComponentValue {
        &self.value
    }

    #[must_use]
    pub fn entity_references(&self) -> &[UnavailableEntityReference] {
        &self.entity_references
    }

    #[must_use]
    pub fn schema_declared_asset_references(&self) -> &[DeclaredAssetReference] {
        &self.schema_declared_asset_references
    }

    #[must_use]
    pub const fn readiness(&self) -> AuthoringRecordReadiness {
        self.readiness
    }
}

/// Canonical preservation barrier for an unavailable component-targeting patch operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailablePatchTarget {
    source: AuthoringDocumentSource,
    document_kind: AuthoringDocumentKind,
    anchor: SceneEntityId,
    anchor_placement: Option<crate::ScenePlacement>,
    operation_path: Vec<usize>,
    target_kind: AuthoringPatchTargetKind,
    entity: SceneEntityId,
    component: ComponentTypeId,
    version: Option<ComponentSchemaVersion>,
    field: Option<ComponentFieldId>,
    readiness: AuthoringRecordReadiness,
}

impl UnavailablePatchTarget {
    #[must_use]
    pub const fn anchor_placement(&self) -> Option<&crate::ScenePlacement> {
        self.anchor_placement.as_ref()
    }
    #[must_use]
    pub const fn source(&self) -> &AuthoringDocumentSource {
        &self.source
    }

    #[must_use]
    pub const fn document_kind(&self) -> AuthoringDocumentKind {
        self.document_kind
    }

    #[must_use]
    pub const fn anchor(&self) -> &SceneEntityId {
        &self.anchor
    }

    #[must_use]
    pub fn operation_path(&self) -> &[usize] {
        &self.operation_path
    }

    #[must_use]
    pub const fn target_kind(&self) -> AuthoringPatchTargetKind {
        self.target_kind
    }

    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    #[must_use]
    pub const fn version(&self) -> Option<ComponentSchemaVersion> {
        self.version
    }

    #[must_use]
    pub const fn field(&self) -> Option<&ComponentFieldId> {
        self.field.as_ref()
    }

    #[must_use]
    pub const fn readiness(&self) -> AuthoringRecordReadiness {
        self.readiness
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnavailableRecordManifestEntry {
    Record(UnavailableRecord),
    PatchTarget(UnavailablePatchTarget),
}

/// Complete ordered semantic manifest used by degraded mutation and Save/Reopen gates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnavailableRecordManifest {
    record_count: usize,
    patch_target_count: usize,
    entries: Vec<UnavailableRecordManifestEntry>,
}

impl UnavailableRecordManifest {
    fn new(entries: Vec<UnavailableRecordManifestEntry>) -> Self {
        let record_count = entries
            .iter()
            .filter(|entry| matches!(entry, UnavailableRecordManifestEntry::Record(_)))
            .count();
        let patch_target_count = entries.len().saturating_sub(record_count);
        Self {
            record_count,
            patch_target_count,
            entries,
        }
    }

    #[must_use]
    pub const fn record_count(&self) -> usize {
        self.record_count
    }

    #[must_use]
    pub const fn patch_target_count(&self) -> usize {
        self.patch_target_count
    }

    #[must_use]
    pub const fn total_count(&self) -> usize {
        self.record_count + self.patch_target_count
    }

    #[must_use]
    pub fn entries(&self) -> &[UnavailableRecordManifestEntry] {
        &self.entries
    }
}

/// Compatibility evidence for a trusted native or finite engine-owned data Adapter.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringAdapterReceipt {
    catalog: AuthoringAdapterCatalog,
    identity: AuthoringAdapterIdentity,
    adapter_generation: NonZeroU64,
    components: BTreeSet<ComponentTypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoringAdapterReceiptError {
    EmptyIdentity,
    ZeroIdentityVersion,
    ZeroGeneration,
    EmptyCoverage,
    DuplicateComponent(ComponentTypeId),
}

impl Display for AuthoringAdapterReceiptError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentity => {
                formatter.write_str("authoring Adapter identity must not be empty")
            }
            Self::ZeroIdentityVersion => {
                formatter.write_str("authoring Adapter identity version must be non-zero")
            }
            Self::ZeroGeneration => {
                formatter.write_str("authoring Adapter generation must be non-zero")
            }
            Self::EmptyCoverage => {
                formatter.write_str("authoring Adapter coverage must not be empty")
            }
            Self::DuplicateComponent(component) => write!(
                formatter,
                "authoring Adapter coverage repeats component '{component}'"
            ),
        }
    }
}

impl Error for AuthoringAdapterReceiptError {}

impl AuthoringAdapterReceipt {
    #[doc(hidden)]
    pub fn from_trusted_provider(
        contribution: ComponentSchemaContributionReceipt,
        adapter_id: &'static str,
        adapter_version: u32,
        adapter_generation: u64,
        components: impl IntoIterator<Item = ComponentTypeId>,
    ) -> Result<Self, AuthoringAdapterReceiptError> {
        if adapter_id.is_empty() {
            return Err(AuthoringAdapterReceiptError::EmptyIdentity);
        }
        if adapter_version == 0 {
            return Err(AuthoringAdapterReceiptError::ZeroIdentityVersion);
        }
        let adapter_generation = NonZeroU64::new(adapter_generation)
            .ok_or(AuthoringAdapterReceiptError::ZeroGeneration)?;
        let mut coverage = BTreeSet::new();
        for component in components {
            if !coverage.insert(component.clone()) {
                return Err(AuthoringAdapterReceiptError::DuplicateComponent(component));
            }
        }
        if coverage.is_empty() {
            return Err(AuthoringAdapterReceiptError::EmptyCoverage);
        }
        Ok(Self {
            catalog: AuthoringAdapterCatalog::Native(contribution),
            identity: AuthoringAdapterIdentity {
                id: adapter_id,
                version: adapter_version,
            },
            adapter_generation,
            components: coverage,
        })
    }

    #[must_use]
    pub const fn contribution(&self) -> Option<ComponentSchemaContributionReceipt> {
        match self.catalog {
            AuthoringAdapterCatalog::Native(contribution) => Some(contribution),
            AuthoringAdapterCatalog::Data { .. } => None,
        }
    }

    /// Installs the engine's finite data Adapter, without granting executable trust.
    #[doc(hidden)]
    pub fn from_schema_owner(
        owner: &AuthoringSchemaBundleOwner,
        native_coverage: &BTreeSet<ComponentTypeId>,
    ) -> Option<Self> {
        let components: BTreeSet<_> = owner
            .current()
            .components()
            .iter()
            .filter(|schema| {
                !native_coverage.contains(schema.id()) && supports_data_authoring(schema)
            })
            .map(|schema| schema.id().clone())
            .collect();
        if components.is_empty() {
            return None;
        }
        Some(Self {
            catalog: AuthoringAdapterCatalog::Data {
                owner: owner.owner_id().clone(),
                generation: owner.current().generation(),
                fingerprint: owner.current().fingerprint(),
                predecessor: owner.current().predecessor().copied(),
            },
            identity: AuthoringAdapterIdentity {
                id: "nara.schema-data-authoring",
                version: 1,
            },
            adapter_generation: NonZeroU64::MIN,
            components,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> AuthoringAdapterIdentity {
        self.identity
    }

    #[must_use]
    pub const fn adapter_generation(&self) -> u64 {
        self.adapter_generation.get()
    }

    #[must_use]
    pub fn components(&self) -> impl ExactSizeIterator<Item = &ComponentTypeId> {
        self.components.iter()
    }
}

fn supports_data_authoring(schema: &ComponentSchema) -> bool {
    schema.has_capability(ComponentCapability::Scene)
        && schema.has_capability(ComponentCapability::Inspect)
        && !schema.fields.is_empty()
        && schema
            .fields
            .iter()
            .all(|field| supports_data_shape(field.shape()))
}

fn supports_data_shape(shape: &ComponentValueShape) -> bool {
    if shape.validate().is_err() {
        return false;
    }
    let mut pending = vec![shape];
    while let Some(shape) = pending.pop() {
        match shape {
            ComponentValueShape::Opaque { .. } => return false,
            ComponentValueShape::Record { members } => {
                pending.extend(members.iter().map(|member| &member.shape));
            }
            ComponentValueShape::List { item } => pending.push(item),
            ComponentValueShape::Map { value } | ComponentValueShape::Optional { value } => {
                pending.push(value);
            }
            ComponentValueShape::Scalar { .. }
            | ComponentValueShape::EntityRef
            | ComponentValueShape::AssetRef => {}
        }
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringDocumentPublicationError {
    diagnostics: Box<DiagnosticReport>,
}

impl AuthoringDocumentPublicationError {
    fn new(diagnostics: DiagnosticReport) -> Self {
        Self {
            diagnostics: Box::new(diagnostics),
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    #[must_use]
    pub fn into_diagnostics(self) -> DiagnosticReport {
        *self.diagnostics
    }
}

impl Display for AuthoringDocumentPublicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "persistent authoring document admission failed with {} error(s)",
            self.diagnostics.stats().observed_errors()
        )
    }
}

impl Error for AuthoringDocumentPublicationError {}

/// Failure to reopen a decoded document through an existing authoring session.
#[derive(Debug)]
pub enum AuthoringSessionReopenError {
    /// The session was created through the native publication path and owns no authoring basis.
    ClassificationBasisUnavailable,
    /// The reopened candidate failed registry-free authoring classification.
    Publication(AuthoringDocumentPublicationError),
}

impl Display for AuthoringSessionReopenError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClassificationBasisUnavailable => formatter.write_str(
                "authoring reopen requires a session created from a published authoring document",
            ),
            Self::Publication(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for AuthoringSessionReopenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ClassificationBasisUnavailable => None,
            Self::Publication(error) => Some(error),
        }
    }
}

impl From<AuthoringDocumentPublicationError> for AuthoringSessionReopenError {
    fn from(error: AuthoringDocumentPublicationError) -> Self {
        Self::Publication(error)
    }
}

/// Move-only Scene authoring candidate.
#[derive(Debug)]
pub struct SceneAuthoringDocumentCandidate {
    document: SceneDocument,
    state: AuthoringDocumentState,
}

impl SceneAuthoringDocumentCandidate {
    #[must_use]
    pub const fn document(&self) -> &SceneDocument {
        &self.document
    }

    #[must_use]
    pub const fn readiness(&self) -> AuthoringDocumentReadiness {
        self.state.readiness
    }

    #[must_use]
    pub fn publish(self) -> PublishedAuthoringSceneDocument {
        PublishedAuthoringSceneDocument {
            document: self.document,
            state: self.state,
        }
    }
}

/// Move-only Prefab authoring candidate.
#[derive(Debug)]
pub struct PrefabAuthoringDocumentCandidate {
    document: PrefabDocument,
    state: AuthoringDocumentState,
}

impl PrefabAuthoringDocumentCandidate {
    #[must_use]
    pub const fn document(&self) -> &PrefabDocument {
        &self.document
    }

    #[must_use]
    pub const fn readiness(&self) -> AuthoringDocumentReadiness {
        self.state.readiness
    }

    #[must_use]
    pub fn publish(self) -> PublishedAuthoringPrefabDocument {
        PublishedAuthoringPrefabDocument {
            document: self.document,
            state: self.state,
        }
    }
}

#[derive(Debug)]
pub struct PublishedAuthoringSceneDocument {
    document: SceneDocument,
    state: AuthoringDocumentState,
}

impl PublishedAuthoringSceneDocument {
    #[must_use]
    pub const fn document(&self) -> &SceneDocument {
        &self.document
    }

    #[must_use]
    pub const fn readiness(&self) -> AuthoringDocumentReadiness {
        self.state.readiness
    }

    #[must_use]
    pub fn record_facts(&self) -> &[AuthoringRecordFact] {
        &self.state.record_facts
    }

    #[must_use]
    pub const fn unavailable_manifest(&self) -> &UnavailableRecordManifest {
        &self.state.unavailable_manifest
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.state.diagnostics
    }

    pub(crate) fn into_parts(self) -> (SceneDocument, AuthoringDocumentState) {
        (self.document, self.state)
    }
}

/// Forks one published Scene source into an independent Editor working owner.
#[must_use]
pub fn fork_published_authoring_scene_document(
    published: &PublishedAuthoringSceneDocument,
) -> PublishedAuthoringSceneDocument {
    PublishedAuthoringSceneDocument {
        document: published.document.clone(),
        state: published.state.clone(),
    }
}

#[derive(Debug)]
pub struct PublishedAuthoringPrefabDocument {
    document: PrefabDocument,
    state: AuthoringDocumentState,
}

impl PublishedAuthoringPrefabDocument {
    #[must_use]
    pub const fn document(&self) -> &PrefabDocument {
        &self.document
    }

    #[must_use]
    pub const fn readiness(&self) -> AuthoringDocumentReadiness {
        self.state.readiness
    }

    #[must_use]
    pub fn record_facts(&self) -> &[AuthoringRecordFact] {
        &self.state.record_facts
    }

    #[must_use]
    pub const fn unavailable_manifest(&self) -> &UnavailableRecordManifest {
        &self.state.unavailable_manifest
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.state.diagnostics
    }

    pub(crate) fn into_parts(self) -> (PrefabDocument, AuthoringDocumentState) {
        (self.document, self.state)
    }
}

/// Forks one published source into an independent Editor working owner.
#[must_use]
pub fn fork_published_authoring_prefab_document(
    published: &PublishedAuthoringPrefabDocument,
) -> PublishedAuthoringPrefabDocument {
    PublishedAuthoringPrefabDocument {
        document: published.document.clone(),
        state: published.state.clone(),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AuthoringDocumentState {
    source: AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    readiness: AuthoringDocumentReadiness,
    record_facts: Vec<AuthoringRecordFact>,
    unavailable_manifest: UnavailableRecordManifest,
    diagnostics: DiagnosticReport,
    basis: Arc<AuthoringClassificationBasis>,
    limits: SceneFileLimits,
}

impl AuthoringDocumentState {
    pub(crate) fn schema_basis(&self) -> AuthoringSchemaBasis {
        AuthoringSchemaBasis(Arc::clone(&self.basis))
    }

    pub(crate) fn catalog_components(&self) -> impl Iterator<Item = (&ComponentSchema, bool)> {
        self.basis
            .schemas
            .values()
            .map(|entry| (&entry.schema, entry.adapter.is_some()))
    }

    pub(crate) fn initial_component(
        &self,
        component: &ComponentTypeId,
    ) -> Option<SceneComponentRecord> {
        let entry = self.basis.schemas.get(component)?;
        if entry.adapter.is_none() || !component_allows_composition(&entry.schema) {
            return None;
        }
        Some(
            SceneComponentRecord::new(
                entry.schema.version(),
                entry.schema.authoring_initial_value()?.clone(),
            )
            .with_owner(entry.owner.clone()),
        )
    }
    pub(crate) const fn readiness(&self) -> AuthoringDocumentReadiness {
        self.readiness
    }

    pub(crate) fn record_facts(&self) -> &[AuthoringRecordFact] {
        &self.record_facts
    }

    pub(crate) fn available_component_schema(
        &self,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        version: ComponentSchemaVersion,
    ) -> Option<&ComponentSchema> {
        let available = self.record_facts.iter().any(|fact| {
            fact.readiness == AuthoringRecordReadiness::Available
                && &fact.component == component
                && matches!(
                    &fact.location,
                    AuthoringRecordLocation::EntityComponent { entity: owner } if owner == entity
                )
        });
        if !available {
            return None;
        }
        self.basis
            .schemas
            .get(component)
            .map(|entry| &entry.schema)
            .filter(|schema| schema.version() == version)
    }

    pub(crate) const fn unavailable_manifest(&self) -> &UnavailableRecordManifest {
        &self.unavailable_manifest
    }

    pub(crate) const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    pub(crate) fn readmission(
        &self,
        history_limits: crate::SceneAuthoringHistoryLimits,
    ) -> AuthoringDocumentReadmission {
        AuthoringDocumentReadmission {
            source: self.source.clone(),
            kind: self.kind,
            basis: Arc::clone(&self.basis),
            history_limits,
            expected_readiness: self.readiness,
            expected_unavailable_manifest: self.unavailable_manifest.clone(),
        }
    }
}

pub(crate) struct PreparedSceneAuthoringPatch {
    pub(crate) document: SceneDocument,
    pub(crate) state: AuthoringDocumentState,
    pub(crate) patch: ScenePatchDocument,
    pub(crate) report: ScenePatchReport,
}

pub(crate) struct PreparedPrefabAuthoringPatch {
    pub(crate) document: PrefabDocument,
    pub(crate) state: AuthoringDocumentState,
    pub(crate) patch: ScenePatchDocument,
    pub(crate) report: ScenePatchReport,
}

#[derive(Debug)]
struct AuthoringClassificationBasis {
    schemas: BTreeMap<ComponentTypeId, AuthoringSchemaEntry>,
}

#[derive(Debug, Clone)]
struct AuthoringSchemaEntry {
    owner: PersistentComponentSchemaOwnerId,
    schema: ComponentSchema,
    adapter: Option<AuthoringAdapterEvidence>,
}

impl SceneDocumentCandidate {
    pub fn admit_authoring(
        self,
        source: AuthoringDocumentSource,
        bundle: &AuthoringSchemaBundle,
        adapters: &[AuthoringAdapterReceipt],
    ) -> Result<SceneAuthoringDocumentCandidate, AuthoringDocumentPublicationError> {
        let (document, limits) = self.into_authoring_parts();
        let basis = Arc::new(AuthoringClassificationBasis::new(bundle, adapters)?);
        let state = classify_scene_document(
            &document,
            source,
            AuthoringDocumentKind::Scene,
            basis,
            limits,
        )?;
        Ok(SceneAuthoringDocumentCandidate { document, state })
    }
}

impl PrefabDocumentCandidate {
    pub fn admit_authoring(
        self,
        source: AuthoringDocumentSource,
        bundle: &AuthoringSchemaBundle,
        adapters: &[AuthoringAdapterReceipt],
    ) -> Result<PrefabAuthoringDocumentCandidate, AuthoringDocumentPublicationError> {
        let (document, limits) = self.into_authoring_parts();
        let basis = Arc::new(AuthoringClassificationBasis::new(bundle, adapters)?);
        let state = classify_scene_document(
            &document.instantiate(),
            source,
            AuthoringDocumentKind::Prefab,
            basis,
            limits,
        )?;
        Ok(PrefabAuthoringDocumentCandidate { document, state })
    }
}

impl AuthoringClassificationBasis {
    fn new(
        bundle: &AuthoringSchemaBundle,
        adapters: &[AuthoringAdapterReceipt],
    ) -> Result<Self, AuthoringDocumentPublicationError> {
        let mut schemas = BTreeMap::new();
        for owner in bundle.owners() {
            for schema in owner.current().components() {
                schemas.insert(
                    schema.id().clone(),
                    AuthoringSchemaEntry {
                        owner: owner.owner_id().clone(),
                        schema: schema.clone(),
                        adapter: None,
                    },
                );
            }
        }

        for adapter in adapters {
            let matching_owner = bundle
                .owners()
                .iter()
                .find(|owner| adapter.catalog.matches(owner));
            let Some(matching_owner) = matching_owner else {
                continue;
            };
            for component in adapter.components() {
                let Some(entry) = schemas.get_mut(component) else {
                    return Err(rejection(with_public_locator(
                        diagnostic_error(
                            "scene.authoring-adapter-component-unknown",
                            "Authoring Adapter covers a component absent from its exact catalog",
                        ),
                        "component-id",
                        component.as_str(),
                    )));
                };
                if entry.owner != *matching_owner.owner_id() {
                    return Err(rejection(with_public_locator(
                        diagnostic_error(
                            "scene.authoring-adapter-owner-mismatch",
                            "Authoring Adapter component coverage belongs to another schema owner",
                        ),
                        "component-id",
                        component.as_str(),
                    )));
                }
                if entry.adapter.is_some() {
                    return Err(rejection(with_public_locator(
                        diagnostic_error(
                            "scene.authoring-adapter-coverage-conflict",
                            "More than one Authoring Adapter covers the same component",
                        ),
                        "component-id",
                        component.as_str(),
                    )));
                }
                entry.adapter = Some(AuthoringAdapterEvidence {
                    identity: adapter.identity,
                    adapter_generation: adapter.adapter_generation,
                    catalog: adapter.catalog.clone(),
                });
            }
        }

        Ok(Self { schemas })
    }

    fn classify_record(
        &self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> Result<ClassifiedRecord, AuthoringDocumentPublicationError> {
        let owner = record
            .owner()
            .map_or(AuthoringRecordOwnerClaim::Unclaimed, |owner| {
                AuthoringRecordOwnerClaim::Declared(owner.clone())
            });
        let entry = self.schemas.get(component).filter(|entry| {
            record
                .owner()
                .is_none_or(|declared| declared == &entry.owner)
        });
        self.classify(component, record.version, owner, entry)
    }

    fn classify_target(
        &self,
        component: &ComponentTypeId,
        version: Option<ComponentSchemaVersion>,
    ) -> Result<ClassifiedRecord, AuthoringDocumentPublicationError> {
        if let Some(version) = version {
            return self.classify(
                component,
                version,
                AuthoringRecordOwnerClaim::Unclaimed,
                self.schemas.get(component),
            );
        }
        let Some(entry) = self.schemas.get(component) else {
            return Ok(ClassifiedRecord {
                owner: AuthoringRecordOwnerClaim::Unclaimed,
                resolved_owner: None,
                readiness: AuthoringRecordReadiness::UnknownSchema,
                adapter: None,
                schema: None,
            });
        };
        if !entry.schema.has_capability(ComponentCapability::Scene) {
            return Err(rejection(with_public_locator(
                diagnostic_error(
                    "scene.authoring-component-not-scene-capable",
                    "Persistent Scene or Prefab operation targets a component without Scene capability",
                ),
                "component-id",
                component.as_str(),
            )));
        }
        let readiness = if entry.adapter.is_some() {
            AuthoringRecordReadiness::Available
        } else {
            AuthoringRecordReadiness::KnownUnbound
        };
        Ok(ClassifiedRecord {
            owner: AuthoringRecordOwnerClaim::Unclaimed,
            resolved_owner: Some(entry.owner.clone()),
            readiness,
            adapter: entry.adapter.clone(),
            schema: Some(entry.schema.clone()),
        })
    }

    fn classify(
        &self,
        component: &ComponentTypeId,
        version: ComponentSchemaVersion,
        owner: AuthoringRecordOwnerClaim,
        entry: Option<&AuthoringSchemaEntry>,
    ) -> Result<ClassifiedRecord, AuthoringDocumentPublicationError> {
        let Some(entry) = entry else {
            return Ok(ClassifiedRecord {
                owner,
                resolved_owner: None,
                readiness: AuthoringRecordReadiness::UnknownSchema,
                adapter: None,
                schema: None,
            });
        };
        if !entry.schema.has_capability(ComponentCapability::Scene) {
            return Err(rejection(with_public_locator(
                diagnostic_error(
                    "scene.authoring-component-not-scene-capable",
                    "Persistent Scene or Prefab record uses a component without Scene capability",
                ),
                "component-id",
                component.as_str(),
            )));
        }
        let readiness = if version > entry.schema.version() {
            AuthoringRecordReadiness::FutureSchema
        } else if version < entry.schema.version() {
            AuthoringRecordReadiness::Unmigratable
        } else if entry.adapter.is_some() {
            AuthoringRecordReadiness::Available
        } else {
            AuthoringRecordReadiness::KnownUnbound
        };
        Ok(ClassifiedRecord {
            owner,
            resolved_owner: Some(entry.owner.clone()),
            readiness,
            adapter: readiness
                .is_available()
                .then(|| entry.adapter.clone())
                .flatten(),
            schema: Some(entry.schema.clone()),
        })
    }
}

struct ClassifiedRecord {
    owner: AuthoringRecordOwnerClaim,
    resolved_owner: Option<PersistentComponentSchemaOwnerId>,
    readiness: AuthoringRecordReadiness,
    adapter: Option<AuthoringAdapterEvidence>,
    schema: Option<ComponentSchema>,
}

struct ClassificationContext<'a> {
    source: &'a AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    basis: &'a AuthoringClassificationBasis,
    record_facts: Vec<AuthoringRecordFact>,
    manifest: Vec<UnavailableRecordManifestEntry>,
    placements: BTreeMap<SceneEntityId, crate::ScenePlacement>,
    namespace: BTreeSet<&'a SceneEntityId>,
    limits: SceneFileLimits,
}

fn classify_scene_document(
    document: &SceneDocument,
    source: AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    basis: Arc<AuthoringClassificationBasis>,
    limits: SceneFileLimits,
) -> Result<AuthoringDocumentState, AuthoringDocumentPublicationError> {
    let structure = validate_scene_structure(document);
    if structure.has_errors() {
        return Err(AuthoringDocumentPublicationError::new(structure));
    }

    let (record_facts, manifest) = {
        let mut context = ClassificationContext {
            source: &source,
            kind,
            basis: &basis,
            record_facts: Vec::new(),
            manifest: Vec::new(),
            placements: crate::patch::authoring_entity_placements(
                &document.roots,
                &document.entities,
            ),
            namespace: document.entities.iter().map(|entity| &entity.id).collect(),
            limits,
        };
        for entity in &document.entities {
            classify_entity_components(entity, AuthoringRecordOrigin::Direct, &mut context)?;
        }
        (context.record_facts, context.manifest)
    };
    let unavailable_manifest = UnavailableRecordManifest::new(manifest);
    let diagnostics = unavailable_record_diagnostics(unavailable_manifest.entries());
    let readiness = if unavailable_manifest.total_count() == 0 {
        AuthoringDocumentReadiness::Complete
    } else {
        AuthoringDocumentReadiness::Degraded
    };
    Ok(AuthoringDocumentState {
        source,
        kind,
        readiness,
        record_facts,
        unavailable_manifest,
        diagnostics,
        basis,
        limits,
    })
}

fn unavailable_record_diagnostics(entries: &[UnavailableRecordManifestEntry]) -> DiagnosticReport {
    let mut diagnostics = DiagnosticReport::default();
    for entry in entries {
        let (source, component, readiness, location) = match entry {
            UnavailableRecordManifestEntry::Record(record) => (
                record.source(),
                record.component(),
                record.readiness(),
                record.location(),
            ),
            UnavailableRecordManifestEntry::PatchTarget(target) => {
                let diagnostic = unavailable_readiness_diagnostic(target.readiness());
                let diagnostic = with_asset_ref(
                    with_public_locator(
                        with_public_locator(
                            diagnostic,
                            "component-id",
                            target.component().as_str(),
                        ),
                        "entity-id",
                        target.entity().as_str(),
                    ),
                    "source",
                    target.source().as_asset_ref(),
                );
                let diagnostic =
                    with_public_locator(diagnostic, "anchor-id", target.anchor().as_str());
                diagnostics.push(with_operation_path(diagnostic, target.operation_path()));
                continue;
            }
        };
        let diagnostic = with_asset_ref(
            with_public_locator(
                unavailable_readiness_diagnostic(readiness),
                "component-id",
                component.as_str(),
            ),
            "source",
            source.as_asset_ref(),
        );
        diagnostics.push(with_authoring_record_location(diagnostic, location));
    }
    diagnostics
}

fn unavailable_readiness_diagnostic(
    readiness: AuthoringRecordReadiness,
) -> nara_diagnostic::Diagnostic {
    match readiness {
        AuthoringRecordReadiness::KnownUnbound => diagnostic_warning(
            "scene.authoring-known-unbound",
            "Persistent component has known schema but no compatible authoring Adapter",
        ),
        AuthoringRecordReadiness::UnknownSchema => diagnostic_warning(
            "scene.authoring-unknown-schema",
            "Persistent component schema is unavailable to the authoring project",
        ),
        AuthoringRecordReadiness::Unmigratable => diagnostic_warning(
            "scene.authoring-unmigratable",
            "Persistent component requires a migration unavailable to generic authoring",
        ),
        AuthoringRecordReadiness::FutureSchema => diagnostic_warning(
            "scene.authoring-future-schema",
            "Persistent component uses a schema version newer than the authoring bundle",
        ),
        AuthoringRecordReadiness::Available => {
            unreachable!("available records are never retained in the unavailable-record manifest")
        }
    }
}

fn with_authoring_record_location(
    diagnostic: nara_diagnostic::Diagnostic,
    location: &AuthoringRecordLocation,
) -> nara_diagnostic::Diagnostic {
    match location {
        AuthoringRecordLocation::EntityComponent { entity } => {
            with_public_locator(diagnostic, "entity-id", entity.as_str())
        }
        AuthoringRecordLocation::PatchComponent {
            anchor,
            operation_path,
            entity,
            ..
        } => with_operation_path(
            with_public_locator(
                with_public_locator(diagnostic, "anchor-id", anchor.as_str()),
                "entity-id",
                entity.as_str(),
            ),
            operation_path,
        ),
    }
}

fn with_operation_path(
    diagnostic: nara_diagnostic::Diagnostic,
    operation_path: &[usize],
) -> nara_diagnostic::Diagnostic {
    let mut encoded = String::new();
    for (index, segment) in operation_path.iter().enumerate() {
        if index > 0 {
            encoded.push('.');
        }
        encoded.push_str(&segment.to_string());
    }
    with_public_locator(diagnostic, "operation-path", &encoded)
}

#[derive(Clone)]
enum AuthoringRecordOrigin {
    Direct,
    Patch {
        anchor: SceneEntityId,
        operation_path: Vec<usize>,
        slot: AuthoringPatchRecordSlot,
        placement: Option<crate::ScenePlacement>,
    },
}

fn classify_entity_components(
    entity: &SceneEntityRecord,
    origin: AuthoringRecordOrigin,
    context: &mut ClassificationContext<'_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let placement = match &origin {
        AuthoringRecordOrigin::Direct => context.placements.get(&entity.id).cloned(),
        AuthoringRecordOrigin::Patch { placement, .. } => placement.clone(),
    };
    for (component, record) in &entity.components {
        let location = match &origin {
            AuthoringRecordOrigin::Direct => AuthoringRecordLocation::EntityComponent {
                entity: entity.id.clone(),
            },
            AuthoringRecordOrigin::Patch {
                anchor,
                operation_path,
                slot,
                ..
            } => AuthoringRecordLocation::PatchComponent {
                anchor: anchor.clone(),
                operation_path: operation_path.clone(),
                slot: slot.clone(),
                entity: entity.id.clone(),
            },
        };
        classify_complete_record(location, placement.clone(), component, record, context)?;
    }
    if let Some(prefab) = &entity.prefab {
        let (anchor, operation_path) = match origin {
            AuthoringRecordOrigin::Direct => (entity.id.clone(), Vec::new()),
            AuthoringRecordOrigin::Patch {
                anchor,
                operation_path,
                ..
            } => (anchor, operation_path),
        };
        classify_patch(&prefab.overrides, &anchor, &operation_path, context)?;
    }
    Ok(())
}

fn classify_complete_record(
    location: AuthoringRecordLocation,
    placement: Option<crate::ScenePlacement>,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    context: &mut ClassificationContext<'_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let classified = context.basis.classify_record(component, record)?;
    if classified.readiness.is_available() {
        let schema = classified
            .schema
            .as_ref()
            .expect("available authoring record retains its current schema");
        let owner = classified
            .resolved_owner
            .as_ref()
            .expect("available authoring record retains its admitted schema owner");
        let effective = effective_component_value(
            schema,
            &record.value,
            context.limits.component_materialization_limits(),
        )
        .map_err(|error| {
            rejection(with_codec_error(
                with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.authoring-component-value-invalid",
                            "Persistent authoring record exceeds its effective-value policy",
                        ),
                        "schema-owner",
                        owner.as_str(),
                    ),
                    "component-id",
                    component.as_str(),
                ),
                &error,
                Some(schema),
            ))
        })?;
        nara_reflect::validate_component_value(schema, &effective).map_err(|error| {
            rejection(with_codec_error(
                with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.authoring-component-value-invalid",
                            "Persistent authoring record does not satisfy its declared schema",
                        ),
                        "schema-owner",
                        owner.as_str(),
                    ),
                    "component-id",
                    component.as_str(),
                ),
                &error,
                Some(schema),
            ))
        })?;
        if matches!(&location, AuthoringRecordLocation::EntityComponent { .. }) {
            validate_effective_local_references(
                schema,
                &effective,
                |entity| context.namespace.contains(entity),
                reference_traversal_limits(context.limits.component_materialization_limits()),
            )
            .map_err(|_| {
                rejection(with_public_locator(
                    diagnostic_error(
                        "scene.authoring-local-reference-invalid",
                        "Persistent authoring references do not close over their source namespace",
                    ),
                    "component-id",
                    component.as_str(),
                ))
            })?;
        }
    }
    let schema_declared_asset_references = if classified
        .schema
        .as_ref()
        .is_none_or(|schema| record.version != schema.version())
    {
        Vec::new()
    } else {
        collect_declared_asset_references_from_schema_with_limits(
            classified
                .schema
                .as_ref()
                .expect("a current known record retains its schema"),
            record.version,
            &record.value,
            context.limits.component_materialization_limits(),
            reference_traversal_limits(context.limits.component_materialization_limits()),
        )
        .map_err(|_| {
            rejection(with_public_locator(
                diagnostic_error(
                    "scene.authoring-declared-asset-reference-invalid",
                    "Persistent authoring record has an invalid schema-declared asset reference",
                ),
                "component-id",
                component.as_str(),
            ))
        })?
    };
    context.record_facts.push(AuthoringRecordFact {
        location: location.clone(),
        owner: classified.owner.clone(),
        resolved_owner: classified.resolved_owner,
        component: component.clone(),
        version: record.version,
        readiness: classified.readiness,
        adapter: classified.adapter,
        schema_declared_asset_references: schema_declared_asset_references.clone(),
    });
    if !classified.readiness.is_available() {
        let entity_references = collect_entity_references(&record.value);
        context
            .manifest
            .push(UnavailableRecordManifestEntry::Record(UnavailableRecord {
                source: context.source.clone(),
                document_kind: context.kind,
                placement,
                anchor_placement: match &location {
                    AuthoringRecordLocation::EntityComponent { .. } => None,
                    AuthoringRecordLocation::PatchComponent { anchor, .. } => {
                        context.placements.get(anchor).cloned()
                    }
                },
                location,
                owner: classified.owner,
                component: component.clone(),
                version: record.version,
                value: record.value.clone(),
                entity_references,
                schema_declared_asset_references,
                readiness: classified.readiness,
            }));
    }
    Ok(())
}

fn collect_entity_references(value: &ComponentValue) -> Vec<UnavailableEntityReference> {
    let mut references = Vec::new();
    collect_entity_references_at(value, &mut Vec::new(), &mut references);
    references
}

#[derive(Clone, Copy)]
enum BorrowedUnavailableValuePathSegment<'a> {
    Field(&'a str),
    Index(usize),
}

fn collect_entity_references_at<'a>(
    value: &'a ComponentValue,
    path: &mut Vec<BorrowedUnavailableValuePathSegment<'a>>,
    references: &mut Vec<UnavailableEntityReference>,
) {
    match value {
        ComponentValue::EntityReference(reference) => {
            references.push(UnavailableEntityReference {
                path: path
                    .iter()
                    .map(|segment| match segment {
                        BorrowedUnavailableValuePathSegment::Field(field) => {
                            UnavailableValuePathSegment::Field((*field).to_owned())
                        }
                        BorrowedUnavailableValuePathSegment::Index(index) => {
                            UnavailableValuePathSegment::Index(*index)
                        }
                    })
                    .collect(),
                reference: reference.clone(),
            });
        }
        ComponentValue::List(values) => {
            for (index, value) in values.iter().enumerate() {
                path.push(BorrowedUnavailableValuePathSegment::Index(index));
                collect_entity_references_at(value, path, references);
                let _ = path.pop();
            }
        }
        ComponentValue::Map(values) => {
            for (field, value) in values {
                path.push(BorrowedUnavailableValuePathSegment::Field(field));
                collect_entity_references_at(value, path, references);
                let _ = path.pop();
            }
        }
        ComponentValue::Null
        | ComponentValue::Bool(_)
        | ComponentValue::I64(_)
        | ComponentValue::U64(_)
        | ComponentValue::F64(_)
        | ComponentValue::String(_) => {}
    }
}

fn classify_patch(
    patch: &ScenePatchDocument,
    anchor: &SceneEntityId,
    parent_path: &[usize],
    context: &mut ClassificationContext<'_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    for (operation_index, operation) in patch.operations.iter().enumerate() {
        let mut operation_path = parent_path.to_vec();
        operation_path.push(operation_index);
        match operation {
            ScenePatchOperation::AddEntity { entity, placement } => classify_entity_components(
                entity,
                AuthoringRecordOrigin::Patch {
                    anchor: anchor.clone(),
                    operation_path,
                    slot: AuthoringPatchRecordSlot::AddEntity,
                    placement: Some(placement.clone()),
                },
                context,
            )?,
            ScenePatchOperation::RestoreEntity {
                entity,
                descendants,
                placement,
            } => {
                classify_entity_components(
                    entity,
                    AuthoringRecordOrigin::Patch {
                        anchor: anchor.clone(),
                        operation_path: operation_path.clone(),
                        slot: AuthoringPatchRecordSlot::RestoreEntity,
                        placement: Some(placement.clone()),
                    },
                    context,
                )?;
                let restored_placements = crate::patch::authoring_entity_placements(
                    std::slice::from_ref(&entity.id),
                    std::iter::once(entity).chain(descendants.iter()),
                );
                for (index, descendant) in descendants.iter().enumerate() {
                    classify_entity_components(
                        descendant,
                        AuthoringRecordOrigin::Patch {
                            anchor: anchor.clone(),
                            operation_path: operation_path.clone(),
                            slot: AuthoringPatchRecordSlot::RestoreDescendant { index },
                            placement: restored_placements.get(&descendant.id).cloned(),
                        },
                        context,
                    )?;
                }
            }
            ScenePatchOperation::AddComponent {
                entity,
                component,
                value,
            } => classify_complete_record(
                AuthoringRecordLocation::PatchComponent {
                    anchor: anchor.clone(),
                    operation_path,
                    slot: AuthoringPatchRecordSlot::AddComponent,
                    entity: entity.clone(),
                },
                None,
                component,
                value,
                context,
            )?,
            ScenePatchOperation::ReplaceComponent {
                entity,
                component,
                value,
            } => classify_complete_record(
                AuthoringRecordLocation::PatchComponent {
                    anchor: anchor.clone(),
                    operation_path,
                    slot: AuthoringPatchRecordSlot::ReplaceComponent,
                    entity: entity.clone(),
                },
                None,
                component,
                value,
                context,
            )?,
            ScenePatchOperation::RemoveComponent { entity, component } => {
                classify_patch_target(
                    anchor,
                    operation_path,
                    AuthoringPatchTargetKind::RemoveComponent,
                    entity,
                    component,
                    None,
                    None,
                    context,
                )?;
            }
            ScenePatchOperation::SetField {
                entity,
                component,
                component_version,
                field,
                ..
            } => classify_patch_target(
                anchor,
                operation_path,
                AuthoringPatchTargetKind::SetField,
                entity,
                component,
                Some(*component_version),
                Some(field),
                context,
            )?,
            ScenePatchOperation::RemoveField {
                entity,
                component,
                component_version,
                field,
            } => classify_patch_target(
                anchor,
                operation_path,
                AuthoringPatchTargetKind::RemoveField,
                entity,
                component,
                Some(*component_version),
                Some(field),
                context,
            )?,
            ScenePatchOperation::SetAssetRefField {
                entity,
                component,
                component_version,
                field,
                ..
            } => classify_patch_target(
                anchor,
                operation_path,
                AuthoringPatchTargetKind::SetAssetRefField,
                entity,
                component,
                Some(*component_version),
                Some(field),
                context,
            )?,
            ScenePatchOperation::RemoveEntity { .. } | ScenePatchOperation::MoveEntity { .. } => {}
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn classify_patch_target(
    anchor: &SceneEntityId,
    operation_path: Vec<usize>,
    target_kind: AuthoringPatchTargetKind,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    version: Option<ComponentSchemaVersion>,
    field: Option<&ComponentFieldId>,
    context: &mut ClassificationContext<'_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    let classified = context.basis.classify_target(component, version)?;
    if classified.readiness.is_available() {
        if let Some(field) = field {
            let schema = &context
                .basis
                .schemas
                .get(component)
                .expect("available component has an admitted schema")
                .schema;
            if !schema
                .fields()
                .iter()
                .any(|candidate| candidate.id() == field)
            {
                return Err(rejection(with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.authoring-patch-field-unknown",
                            "Available authoring patch targets an unknown component field",
                        ),
                        "component-id",
                        component.as_str(),
                    ),
                    "field-id",
                    field.as_str(),
                )));
            }
        }
        return Ok(());
    }
    context
        .manifest
        .push(UnavailableRecordManifestEntry::PatchTarget(
            UnavailablePatchTarget {
                source: context.source.clone(),
                document_kind: context.kind,
                anchor: anchor.clone(),
                anchor_placement: context.placements.get(anchor).cloned(),
                operation_path,
                target_kind,
                entity: entity.clone(),
                component: component.clone(),
                version,
                field: field.cloned(),
                readiness: classified.readiness,
            },
        ));
    Ok(())
}

fn rejection(diagnostic: nara_diagnostic::Diagnostic) -> AuthoringDocumentPublicationError {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic);
    AuthoringDocumentPublicationError::new(diagnostics)
}

fn authoring_reopen_kind_rejection(
    expected: AuthoringDocumentKind,
    actual: AuthoringDocumentKind,
) -> AuthoringDocumentPublicationError {
    rejection(with_public_locator(
        with_public_locator(
            diagnostic_error(
                "scene.authoring-reopen-kind-mismatch",
                "Authoring reopen classification authority belongs to another document kind",
            ),
            "expected-kind",
            match expected {
                AuthoringDocumentKind::Scene => "scene",
                AuthoringDocumentKind::Prefab => "prefab",
            },
        ),
        "actual-kind",
        match actual {
            AuthoringDocumentKind::Scene => "scene",
            AuthoringDocumentKind::Prefab => "prefab",
        },
    ))
}

pub(crate) fn prepare_scene_authoring_patch(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    if patch.operations.iter().any(|operation| {
        !matches!(
            operation,
            ScenePatchOperation::SetField { .. }
                | ScenePatchOperation::SetAssetRefField { .. }
                | ScenePatchOperation::RemoveField { .. }
        )
    }) {
        return prepare_scene_authoring_structure(document, state, patch, history);
    }
    let edits = field_edits_from_patch(patch)?;
    let edit = &edits[0];
    let (mut candidate, inverse) = prepare_field_edits(document.clone(), state, &edits)?;
    field_edits_from_patch(&inverse)?;
    if crate::format::validate_scene_publication_shape(&candidate, state.limits).is_err() {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-format-invalid",
            "Authoring field edit exceeds the persistent Scene format boundary",
        ));
    }
    candidate.canonicalize();
    let candidate_state = classify_scene_document(
        &candidate,
        state.source.clone(),
        state.kind,
        Arc::clone(&state.basis),
        state.limits,
    )
    .map_err(|error| ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics: error.into_diagnostics(),
    })?;
    if candidate_state.unavailable_manifest != state.unavailable_manifest {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-unavailable-manifest-changed",
            "Authoring field edit changed the unavailable-record manifest",
        ));
    }
    Ok(PreparedSceneAuthoringPatch {
        document: candidate,
        state: candidate_state,
        patch: patch.clone(),
        report: ScenePatchReport {
            applied: true,
            inverse: Some(inverse),
            diagnostics: DiagnosticReport::default(),
        },
    })
}

pub(crate) fn prepare_prefab_authoring_patch(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
) -> Result<PreparedPrefabAuthoringPatch, ScenePatchReport> {
    if patch.operations.iter().any(|operation| {
        !matches!(
            operation,
            ScenePatchOperation::SetField { .. }
                | ScenePatchOperation::SetAssetRefField { .. }
                | ScenePatchOperation::RemoveField { .. }
        )
    }) {
        let prepared =
            prepare_scene_authoring_structure(&document.instantiate(), state, patch, history)?;
        let candidate = PrefabDocument::new(prepared.document.roots, prepared.document.entities);
        if crate::format::validate_prefab_publication_shape(&candidate, state.limits).is_err() {
            return Err(authoring_structure_rejection(
                "Authoring structure exceeds the Prefab format boundary",
            ));
        }
        return Ok(PreparedPrefabAuthoringPatch {
            document: candidate,
            state: prepared.state,
            patch: prepared.patch,
            report: prepared.report,
        });
    }
    let edits = field_edits_from_patch(patch)?;
    let edit = &edits[0];
    let scene = document.instantiate();
    let (candidate_scene, inverse) = prepare_field_edits(scene, state, &edits)?;
    field_edits_from_patch(&inverse)?;
    let mut candidate = PrefabDocument::new(
        candidate_scene.roots.clone(),
        candidate_scene.entities.clone(),
    );
    if crate::format::validate_prefab_publication_shape(&candidate, state.limits).is_err() {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-format-invalid",
            "Authoring field edit exceeds the persistent Prefab format boundary",
        ));
    }
    candidate.canonicalize();
    let candidate_state = classify_scene_document(
        &candidate.instantiate(),
        state.source.clone(),
        state.kind,
        Arc::clone(&state.basis),
        state.limits,
    )
    .map_err(|error| ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics: error.into_diagnostics(),
    })?;
    if candidate_state.unavailable_manifest != state.unavailable_manifest {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-unavailable-manifest-changed",
            "Authoring field edit changed the unavailable-record manifest",
        ));
    }
    Ok(PreparedPrefabAuthoringPatch {
        document: candidate,
        state: candidate_state,
        patch: patch.clone(),
        report: ScenePatchReport {
            applied: true,
            inverse: Some(inverse),
            diagnostics: DiagnosticReport::default(),
        },
    })
}

fn component_allows_composition(schema: &ComponentSchema) -> bool {
    schema.has_capability(ComponentCapability::Scene)
        && schema.has_capability(ComponentCapability::Edit)
        && schema
            .fields()
            .iter()
            .all(|field| field.has_capability(ComponentCapability::Edit))
}

struct AuthoringFieldChange {
    entity: SceneEntityId,
    component: ComponentTypeId,
    component_version: ComponentSchemaVersion,
    field: ComponentFieldId,
    value: Option<ComponentValue>,
}

fn field_edits_from_patch(
    patch: &ScenePatchDocument,
) -> Result<Vec<AuthoringFieldChange>, ScenePatchReport> {
    if patch.format_version != ScenePatchDocument::CURRENT_FORMAT_VERSION
        || patch.is_empty()
        || patch.operations.len() > MAX_AUTHORING_FIELD_WRITES
    {
        return Err(authoring_field_patch_rejection());
    }
    let mut fields = BTreeSet::new();
    let mut value_nodes = 0usize;
    let mut logical_bytes = 0usize;
    patch
        .operations
        .iter()
        .map(|operation| {
            let asset_value;
            let (entity, component, component_version, field, value) = match operation {
                ScenePatchOperation::SetField {
                    entity,
                    component,
                    component_version,
                    field,
                    value,
                } => (entity, component, component_version, field, Some(value)),
                ScenePatchOperation::SetAssetRefField {
                    entity,
                    component,
                    component_version,
                    field,
                    asset_ref,
                } => {
                    asset_value = crate::patch::asset_ref_value(asset_ref);
                    (
                        entity,
                        component,
                        component_version,
                        field,
                        Some(&asset_value),
                    )
                }
                ScenePatchOperation::RemoveField {
                    entity,
                    component,
                    component_version,
                    field,
                } => (entity, component, component_version, field, None),
                _ => return Err(authoring_field_patch_rejection()),
            };
            if let Some(value) = &value {
                let cost = value.cost();
                value_nodes = value_nodes.saturating_add(cost.nodes());
                logical_bytes = logical_bytes.saturating_add(cost.logical_bytes());
            }
            if !fields.insert((entity, component, field))
                || value_nodes > MAX_AUTHORING_FIELD_VALUE_NODES
                || logical_bytes > MAX_AUTHORING_FIELD_LOGICAL_BYTES
            {
                return Err(authoring_field_patch_rejection());
            }
            Ok(AuthoringFieldChange {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                field: field.clone(),
                value: value.cloned(),
            })
        })
        .collect()
}

fn authoring_field_patch_rejection() -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic_error(
        "scene.authoring-field-patch-unavailable",
        "Authoring transactions require bounded distinct preservation-safe field changes",
    ));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

fn prepare_field_edits(
    mut candidate: SceneDocument,
    state: &AuthoringDocumentState,
    edits: &[AuthoringFieldChange],
) -> Result<(SceneDocument, ScenePatchDocument), ScenePatchReport> {
    let mut inverse = Vec::with_capacity(edits.len());
    for edit in edits {
        let (next, undo) = prepare_field_edit(candidate, state, edit)?;
        candidate = next;
        inverse.extend(undo.operations);
    }
    inverse.reverse();
    Ok((candidate, ScenePatchDocument::new(inverse)))
}

fn prepare_field_edit(
    mut candidate: SceneDocument,
    state: &AuthoringDocumentState,
    edit: &AuthoringFieldChange,
) -> Result<(SceneDocument, ScenePatchDocument), ScenePatchReport> {
    let available = state.record_facts.iter().any(|fact| {
        fact.readiness == AuthoringRecordReadiness::Available
            && fact.component == edit.component
            && matches!(
                &fact.location,
                AuthoringRecordLocation::EntityComponent { entity } if entity == &edit.entity
            )
    });
    if !available {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-unavailable",
            "Authoring field edit requires an Available direct component record",
        ));
    }

    let Some(schema_entry) = state.basis.schemas.get(&edit.component) else {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-schema-missing",
            "Authoring field edit has no admitted component schema",
        ));
    };
    let Some(entity) = candidate
        .entities
        .iter_mut()
        .find(|entity| entity.id == edit.entity)
    else {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-entity-missing",
            "Authoring field edit targets a missing entity",
        ));
    };
    let Some(record) = entity.components.get_mut(&edit.component) else {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-component-missing",
            "Authoring field edit targets a missing component record",
        ));
    };
    if record.version != schema_entry.schema.version() || edit.component_version != record.version {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-version-mismatch",
            "Authoring field edit requires the current component schema version",
        ));
    }

    let Some(field) = schema_entry
        .schema
        .fields()
        .iter()
        .find(|field| field.id() == &edit.field)
    else {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-field-missing",
            "Authoring field edit targets an unknown component field",
        ));
    };
    if !authoring_field_change_is_preservation_safe(field, edit.value.as_ref()) {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-field-edit-not-preservation-safe",
            "Authoring field edit is outside the preservation-safe value whitelist",
        ));
    }
    let previous = match &edit.value {
        Some(value) => record.value.set_path(field.path(), value.clone()),
        None => record.value.remove_path(field.path()).map(Some),
    }
    .map_err(|_| {
        field_edit_rejection(
            edit,
            "scene.authoring-field-edit-path-invalid",
            "Authoring field edit path cannot be changed in the component value",
        )
    })?;
    let inverse = match previous {
        Some(value) => ScenePatchOperation::SetField {
            entity: edit.entity.clone(),
            component: edit.component.clone(),
            component_version: record.version,
            field: edit.field.clone(),
            value,
        },
        None => ScenePatchOperation::RemoveField {
            entity: edit.entity.clone(),
            component: edit.component.clone(),
            component_version: record.version,
            field: edit.field.clone(),
        },
    };
    Ok((candidate, ScenePatchDocument::new([inverse])))
}

fn authoring_field_change_is_preservation_safe(
    field: &nara_reflect::ComponentFieldSchema,
    value: Option<&ComponentValue>,
) -> bool {
    field.has_capability(ComponentCapability::Edit)
        && !field.has_capability(ComponentCapability::EntityRef)
        && supports_data_shape(field.shape())
        && match value {
            // The complete candidate is validated by classification before publication. Keep
            // value failures there so the shared validator retains its safe code and root field.
            Some(_) => true,
            None => {
                (!field.required || field.default_value.is_some())
                    && matches!(
                        field.path().segments().last(),
                        Some(ComponentFieldPathSegment::Field(_))
                    )
            }
        }
}

fn field_edit_rejection(
    edit: &AuthoringFieldChange,
    code: &'static str,
    summary: &'static str,
) -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    let diagnostic = with_public_locator(
        with_public_locator(
            diagnostic_error(code, summary),
            "entity-id",
            edit.entity.as_str(),
        ),
        "component-id",
        edit.component.as_str(),
    );
    diagnostics.push(with_public_locator(
        diagnostic,
        "field-id",
        edit.field.as_str(),
    ));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

#[cfg(test)]
mod readmission_tests {
    use super::*;
    use crate::{PrefabAuthoringSession, SceneAuthoringHistoryLimits, SceneAuthoringSession};

    fn unavailable_entity() -> SceneEntityRecord {
        SceneEntityRecord::new(SceneEntityId::new("root").unwrap()).with_component(
            ComponentTypeId::new("project.Unavailable"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve this record".to_owned()),
            ),
        )
    }

    #[test]
    fn native_sessions_cannot_capture_authoring_readmission() {
        assert!(
            SceneAuthoringSession::new(SceneDocument::default())
                .authoring_readmission()
                .is_none()
        );
        assert!(
            PrefabAuthoringSession::new(PrefabDocument::default())
                .authoring_readmission()
                .is_none()
        );
    }

    #[test]
    fn owned_scene_readmission_survives_its_session_and_preserves_only_the_exact_manifest() {
        let document = SceneDocument::from_ordered_roots([unavailable_entity()]);
        let bytes = document.to_json_string().unwrap().into_bytes();
        let bundle = AuthoringSchemaBundle::new([]).unwrap();
        let published = SceneDocumentCandidate::decode_json_bytes(&bytes)
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scenes/root.scene.json").unwrap()),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
        let limits = SceneAuthoringHistoryLimits::new(8, 16, 32, 1024, 16, 32, 1024).unwrap();
        let session = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
            published, limits,
        );
        let source_id = session.revision().source_id();
        let admission = session.authoring_readmission().unwrap();
        drop(session);

        let reopened = std::thread::spawn(move || {
            let reopened = admission
                .reopen_scene_candidate(SceneDocumentCandidate::decode_json_bytes(&bytes).unwrap())
                .unwrap();
            assert!(admission.preserves_scene(&reopened));
            let drift = admission
                .reopen_scene_candidate(
                    SceneDocumentCandidate::decode_json_bytes(
                        SceneDocument::default()
                            .to_json_string()
                            .unwrap()
                            .as_bytes(),
                    )
                    .unwrap(),
                )
                .unwrap();
            assert!(!admission.preserves_scene(&drift));
            assert!(
                admission
                    .reopen_prefab_candidate(
                        PrefabDocumentCandidate::decode_json_bytes(
                            PrefabDocument::default()
                                .to_json_string()
                                .unwrap()
                                .as_bytes()
                        )
                        .unwrap()
                    )
                    .is_err()
            );
            reopened
        })
        .join()
        .unwrap();
        assert_eq!(reopened.document(), &document);
        assert_eq!(reopened.history_limits(), limits);
        assert_ne!(reopened.revision().source_id(), source_id);
        assert_eq!(reopened.history_status().undo_depth, 0);
    }

    #[test]
    fn owned_prefab_readmission_survives_its_session_and_preserves_only_the_exact_manifest() {
        let document = PrefabDocument::from_ordered_roots([unavailable_entity()]);
        let bytes = document.to_json_string().unwrap().into_bytes();
        let bundle = AuthoringSchemaBundle::new([]).unwrap();
        let published = PrefabDocumentCandidate::decode_json_bytes(&bytes)
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("prefabs/root.prefab.json").unwrap()),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
        let limits = SceneAuthoringHistoryLimits::new(8, 16, 32, 1024, 16, 32, 1024).unwrap();
        let session = PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
            published, limits,
        );
        let source_id = session.revision().source_id();
        let admission = session.authoring_readmission().unwrap();
        drop(session);

        let reopened = std::thread::spawn(move || {
            let reopened = admission
                .reopen_prefab_candidate(
                    PrefabDocumentCandidate::decode_json_bytes(&bytes).unwrap(),
                )
                .unwrap();
            assert!(admission.preserves_prefab(&reopened));
            let drift = admission
                .reopen_prefab_candidate(
                    PrefabDocumentCandidate::decode_json_bytes(
                        PrefabDocument::default()
                            .to_json_string()
                            .unwrap()
                            .as_bytes(),
                    )
                    .unwrap(),
                )
                .unwrap();
            assert!(!admission.preserves_prefab(&drift));
            assert!(
                admission
                    .reopen_scene_candidate(
                        SceneDocumentCandidate::decode_json_bytes(
                            SceneDocument::default()
                                .to_json_string()
                                .unwrap()
                                .as_bytes()
                        )
                        .unwrap()
                    )
                    .is_err()
            );
            reopened
        })
        .join()
        .unwrap();
        assert_eq!(reopened.document(), &document);
        assert_eq!(reopened.history_limits(), limits);
        assert_ne!(reopened.revision().source_id(), source_id);
        assert_eq!(reopened.history_status().undo_depth, 0);
    }
}
