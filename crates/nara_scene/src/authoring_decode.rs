//! Registry-free Scene and Prefab admission for persistent authoring.

#[cfg(test)]
mod asset_budget_tests;
mod complete_value;
mod conversion;
mod field_candidate;
mod overlay;
mod read;
mod reclassification;
mod retention;
#[cfg(test)]
mod shared_publication_tests;
mod structure;

use complete_value::validate_complete_value;
pub use read::{
    AuthoringComponentView, AuthoringDirectMembers, AuthoringEntityView, AuthoringPropertyValue,
    AuthoringReadError,
};
pub(crate) use retention::asset_references_bytes as retained_asset_references_bytes;
use structure::authoring_structure_rejection;

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    num::NonZeroU64,
    sync::Arc,
};

use nara_asset::AssetRef;
#[cfg(test)]
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{
    AuthoringSchemaBundle, CatalogFingerprint, ComponentCapability, ComponentFieldId,
    ComponentFieldPathSegment, ComponentSchema, ComponentSchemaContributionReceipt,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueShape, DeclaredAssetReference, EntityReference, PersistentComponentSchemaOwnerId,
    collect_declared_asset_references_from_schema_with_limits, effective_component_value,
};

use crate::{
    PrefabDocument, PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstance,
    PrefabPropertyClaim, PrefabPropertyIntent, SceneComponentRecord, SceneDocument,
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
/// Admit one basis from the actual schema bundle and Adapters, then share it across independent
/// Scene and Prefab sources. Clones compare equal while they retain the same admission in this
/// process. Independent basis admissions compare unequal even when their catalog contents match.
/// This is neither a serialized catalog fingerprint nor authority to mutate a document.
#[derive(Clone)]
pub struct AuthoringSchemaBasis(Arc<AuthoringClassificationBasis>);

impl AuthoringSchemaBasis {
    /// Admits the exact current schema catalogs and compatible Adapter coverage once.
    ///
    /// Reuses ordinary authoring admission checks without constructing a native registry or
    /// retaining a document, source identity, mutation authority or history.
    pub fn admit(
        bundle: &AuthoringSchemaBundle,
        adapters: &[AuthoringAdapterReceipt],
    ) -> Result<Self, AuthoringDocumentPublicationError> {
        AuthoringClassificationBasis::new(bundle, adapters).map(|basis| Self(Arc::new(basis)))
    }

    /// Borrows the complete current declaration selected by this exact metadata admission.
    ///
    /// This does not establish record presence/readiness, executable binding or edit authority.
    /// Observation-driven tools can compare their required declaration before interpreting data;
    /// the document-query basis and prepared candidate still require independent revalidation.
    pub fn schema(&self, component: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.0.schemas.get(component).map(|entry| &entry.schema)
    }

    /// Borrows the owner of a selected current declaration, independently of record presence.
    /// Equal declarations under different owners do not identify the same component contract.
    pub fn component_schema_owner(
        &self,
        component: &ComponentTypeId,
    ) -> Option<&PersistentComponentSchemaOwnerId> {
        self.0.schemas.get(component).map(|entry| &entry.owner)
    }

    pub(crate) fn has_authoring_adapter(&self, component: &ComponentTypeId) -> bool {
        self.0
            .schemas
            .get(component)
            .is_some_and(|entry| entry.adapter.is_some())
    }

    pub(crate) fn has_native_preview_adapter(&self, component: &ComponentTypeId) -> bool {
        self.0
            .schemas
            .get(component)
            .and_then(|entry| entry.adapter.as_ref())
            .is_some_and(AuthoringAdapterEvidence::supports_native_preview)
    }
}

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
                SceneAuthoringDocumentCandidate { document, state }.publish(),
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
                PrefabAuthoringDocumentCandidate { document, state }.publish(),
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

/// Schema/Adapter readiness of a persistent record or declarative component/property intent.
/// Overlay readiness does not admit a target, bindings, base or effective activation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthoringRecordReadiness {
    Available,
    KnownUnbound,
    UnknownSchema,
    Unmigratable,
    FutureSchema,
    /// Understood source intent that cannot be interpreted against the current Schema.
    /// Target/base activation is deliberately not part of source classification.
    UnresolvedOverlay,
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
pub enum AuthoringOverlayRecordSlot {
    Addition {
        index: usize,
    },
    Component {
        index: usize,
    },
    Property {
        component_index: usize,
        claim_index: usize,
    },
}

/// Actual nested instance declarations, including the exact lookup and canonical base claims.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthoringOverlayPathSegment {
    Root {
        source: AssetRef,
        base: crate::SourceDocumentKey,
    },
    Addition {
        index: usize,
        entity: SceneEntityId,
        source: AssetRef,
        base: crate::SourceDocumentKey,
    },
    InstanceClaim {
        index: usize,
        target: PrefabEntityTarget,
        source: AssetRef,
        base: crate::SourceDocumentKey,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthoringRecordLocation {
    EntityComponent {
        entity: SceneEntityId,
    },
    OverlayComponent {
        anchor: SceneEntityId,
        overlays: Vec<AuthoringOverlayPathSegment>,
        slot: AuthoringOverlayRecordSlot,
        target: PrefabEntityTarget,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthoringOverlayTargetKind {
    SuppressComponent,
    SetProperty,
    AbsentProperty,
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
    fn matches(&self, owner: &ComponentSchemaOwnerSnapshot) -> bool {
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

/// Immutable Schema/Adapter classification for one complete component record.
/// Overlay facts describe source inventory; the Prefab compiler admits their effective values.
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

/// Maximum ordered operations in one mixed source-authoring transaction.
/// Field edits retain their separate, smaller write/value budgets.
pub const MAX_AUTHORING_EDIT_OPERATIONS: usize = 256;
pub const MAX_AUTHORING_FIELD_WRITES: usize = 8;
pub(crate) const MAX_AUTHORING_FIELD_VALUE_NODES: usize = 256;
pub(crate) const MAX_AUTHORING_FIELD_LOGICAL_BYTES: usize = 64 * 1024;

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

    /// Placement of the source-local entity containing an overlay declaration.
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

/// Preserved component suppression or property declaration without an available interpretation.
///
/// Locations and payloads refer to actual declarative overlay data. Source admission never
/// resolves their effective targets.
#[derive(Debug, Clone, PartialEq)]
pub struct UnavailableOverlayTarget {
    source: AuthoringDocumentSource,
    document_kind: AuthoringDocumentKind,
    location: AuthoringRecordLocation,
    anchor_placement: Option<crate::ScenePlacement>,
    target_kind: AuthoringOverlayTargetKind,
    component: ComponentTypeId,
    version: Option<ComponentSchemaVersion>,
    property: Option<PrefabPropertyClaim>,
    readiness: AuthoringRecordReadiness,
}

impl UnavailableOverlayTarget {
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
    pub fn anchor(&self) -> &SceneEntityId {
        match &self.location {
            AuthoringRecordLocation::OverlayComponent { anchor, .. } => anchor,
            AuthoringRecordLocation::EntityComponent { .. } => {
                unreachable!("overlay targets always retain their declaring anchor")
            }
        }
    }

    #[must_use]
    pub const fn location(&self) -> &AuthoringRecordLocation {
        &self.location
    }

    #[must_use]
    pub fn target(&self) -> &PrefabEntityTarget {
        match &self.location {
            AuthoringRecordLocation::OverlayComponent { target, .. } => target,
            AuthoringRecordLocation::EntityComponent { .. } => {
                unreachable!("overlay targets always retain their authored target")
            }
        }
    }

    #[must_use]
    pub const fn target_kind(&self) -> AuthoringOverlayTargetKind {
        self.target_kind
    }

    #[must_use]
    pub fn entity(&self) -> &SceneEntityId {
        self.target().entity.entity()
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
    pub fn field(&self) -> Option<&ComponentFieldId> {
        self.property.as_ref().map(|property| &property.path.root)
    }

    #[must_use]
    pub const fn property(&self) -> Option<&PrefabPropertyClaim> {
        self.property.as_ref()
    }

    #[must_use]
    pub fn property_path(&self) -> Option<&nara_reflect::ComponentPropertyPath> {
        self.property.as_ref().map(|property| &property.path)
    }

    #[must_use]
    pub fn bindings(&self) -> &[crate::PrefabReferenceBinding] {
        self.property
            .as_ref()
            .map_or(&[], |property| property.bindings.as_slice())
    }

    #[must_use]
    pub fn value(&self) -> Option<&ComponentValue> {
        match &self.property.as_ref()?.intent {
            PrefabPropertyIntent::Set(value) => Some(value),
            PrefabPropertyIntent::Absent => None,
        }
    }

    #[must_use]
    pub const fn readiness(&self) -> AuthoringRecordReadiness {
        self.readiness
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnavailableRecordManifestEntry {
    Record(UnavailableRecord),
    OverlayTarget(UnavailableOverlayTarget),
}

/// Complete semantic preservation manifest used by source mutation and Save/Reopen gates.
/// Unavailable entries determine readiness; complete instance declarations independently guard
/// dormant intent, including declarations whose current Schema is understood.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnavailableRecordManifest {
    record_count: usize,
    overlay_target_count: usize,
    entries: Vec<UnavailableRecordManifestEntry>,
    // Retain each containing instance once, including understood but unresolved declarations.
    // This also preserves bindings, placements, empty base claims and nested detach declarations
    // that do not themselves produce an unavailable component entry.
    overlays: BTreeMap<SceneEntityId, PrefabInstance>,
}

impl UnavailableRecordManifest {
    fn new(
        entries: Vec<UnavailableRecordManifestEntry>,
        overlays: BTreeMap<SceneEntityId, PrefabInstance>,
    ) -> Self {
        let record_count = entries
            .iter()
            .filter(|entry| matches!(entry, UnavailableRecordManifestEntry::Record(_)))
            .count();
        let overlay_target_count = entries.len().saturating_sub(record_count);
        Self {
            record_count,
            overlay_target_count,
            entries,
            overlays,
        }
    }

    #[must_use]
    pub const fn record_count(&self) -> usize {
        self.record_count
    }

    #[must_use]
    pub const fn overlay_target_count(&self) -> usize {
        self.overlay_target_count
    }

    #[must_use]
    pub const fn total_count(&self) -> usize {
        self.record_count + self.overlay_target_count
    }

    #[must_use]
    pub fn entries(&self) -> &[UnavailableRecordManifestEntry] {
        &self.entries
    }

    /// Exact source declarations used by Save/Reopen and source transaction preservation guards.
    /// These are inventory, including dormant intent, not effective activation requirements.
    pub fn overlay_declarations(&self) -> impl Iterator<Item = (&SceneEntityId, &PrefabInstance)> {
        self.overlays.iter()
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
        owner: &ComponentSchemaOwnerSnapshot,
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
            ComponentValueShape::List { item } | ComponentValueShape::KeyedList { item, .. } => {
                pending.push(item)
            }
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
            document: Arc::new(self.document),
            state: Arc::new(self.state),
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
            document: Arc::new(self.document),
            state: Arc::new(self.state),
        }
    }
}

/// Move-only publication retaining one immutable source and its exact classification.
#[derive(Debug)]
pub struct PublishedAuthoringSceneDocument {
    document: Arc<SceneDocument>,
    state: Arc<AuthoringDocumentState>,
}

impl PublishedAuthoringSceneDocument {
    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        &self.document
    }

    #[must_use]
    pub fn readiness(&self) -> AuthoringDocumentReadiness {
        self.state.readiness
    }

    /// Retains the exact admission shared with other Scene or Prefab sources.
    #[must_use]
    pub fn schema_basis(&self) -> AuthoringSchemaBasis {
        self.state.schema_basis()
    }

    #[must_use]
    pub fn record_facts(&self) -> &[AuthoringRecordFact] {
        &self.state.record_facts
    }

    /// Borrows Schema-declared asset inventory from records, property payloads and exposed defaults.
    /// Dormant overlay declarations remain inventory; effective activation owns its actual demand.
    pub fn schema_declared_asset_references(
        &self,
    ) -> impl Iterator<Item = &DeclaredAssetReference> {
        self.state.schema_declared_asset_references()
    }

    #[must_use]
    pub fn unavailable_manifest(&self) -> &UnavailableRecordManifest {
        &self.state.unavailable_manifest
    }

    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.state.diagnostics
    }

    pub(crate) fn into_parts(self) -> (Arc<SceneDocument>, Arc<AuthoringDocumentState>) {
        (self.document, self.state)
    }
}

/// Forks one published Scene source into an independent Editor working owner.
///
/// Shares immutable source and classification handles. Session construction creates independent
/// source identity and history; subsequent edits replace only that Session's handles.
#[must_use]
pub fn fork_published_authoring_scene_document(
    published: &PublishedAuthoringSceneDocument,
) -> PublishedAuthoringSceneDocument {
    PublishedAuthoringSceneDocument {
        document: Arc::clone(&published.document),
        state: Arc::clone(&published.state),
    }
}

/// Move-only publication retaining one immutable nominal Prefab and its exact classification.
#[derive(Debug)]
pub struct PublishedAuthoringPrefabDocument {
    document: Arc<PrefabDocument>,
    state: Arc<AuthoringDocumentState>,
}

impl PublishedAuthoringPrefabDocument {
    pub(crate) fn checked_classification_retained_bytes(&self) -> Option<usize> {
        self.state.checked_retained_bytes()
    }

    #[must_use]
    pub fn document(&self) -> &PrefabDocument {
        &self.document
    }

    #[must_use]
    pub fn readiness(&self) -> AuthoringDocumentReadiness {
        self.state.readiness
    }

    /// Retains the exact admission shared with other Scene or Prefab sources.
    #[must_use]
    pub fn schema_basis(&self) -> AuthoringSchemaBasis {
        self.state.schema_basis()
    }

    #[must_use]
    pub fn record_facts(&self) -> &[AuthoringRecordFact] {
        &self.state.record_facts
    }

    /// Borrows source-classified asset inventory without Host-side Schema interpretation.
    /// Dormant overlay declarations do not independently establish effective activation demand.
    pub fn schema_declared_asset_references(
        &self,
    ) -> impl Iterator<Item = &DeclaredAssetReference> {
        self.state.schema_declared_asset_references()
    }

    #[must_use]
    pub fn unavailable_manifest(&self) -> &UnavailableRecordManifest {
        &self.state.unavailable_manifest
    }

    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.state.diagnostics
    }

    pub(crate) fn into_parts(self) -> (Arc<PrefabDocument>, Arc<AuthoringDocumentState>) {
        (self.document, self.state)
    }
}

/// Forks one published source into an independent Editor working owner.
///
/// Shares immutable source and classification handles. Session construction creates independent
/// source identity and history; subsequent edits replace only that Session's handles.
#[must_use]
pub fn fork_published_authoring_prefab_document(
    published: &PublishedAuthoringPrefabDocument,
) -> PublishedAuthoringPrefabDocument {
    PublishedAuthoringPrefabDocument {
        document: Arc::clone(&published.document),
        state: Arc::clone(&published.state),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AuthoringDocumentState {
    source: AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    readiness: AuthoringDocumentReadiness,
    record_facts: Vec<AuthoringRecordFact>,
    overlay_asset_references: BTreeMap<SceneEntityId, Vec<DeclaredAssetReference>>,
    direct_record_facts: BTreeMap<SceneEntityId, BTreeMap<ComponentTypeId, usize>>,
    unavailable_manifest: UnavailableRecordManifest,
    diagnostics: DiagnosticReport,
    basis: Arc<AuthoringClassificationBasis>,
    limits: SceneFileLimits,
}

#[derive(Clone, Copy)]
pub(crate) struct AuthoringComponentTemplate<'a> {
    pub(crate) schema: &'a ComponentSchema,
    pub(crate) value: &'a ComponentValue,
    owner: &'a PersistentComponentSchemaOwnerId,
}

impl AuthoringComponentTemplate<'_> {
    pub(crate) fn to_record(self) -> SceneComponentRecord {
        SceneComponentRecord::new(self.schema.version(), self.value.clone())
            .with_owner(self.owner.clone())
    }
}

impl AuthoringDocumentState {
    pub(crate) const fn file_limits(&self) -> SceneFileLimits {
        self.limits
    }

    pub(crate) fn source(&self) -> &AuthoringDocumentSource {
        &self.source
    }

    pub(crate) fn schema_declared_asset_references(
        &self,
    ) -> impl Iterator<Item = &DeclaredAssetReference> {
        self.record_facts
            .iter()
            .flat_map(|record| record.schema_declared_asset_references())
            .chain(self.overlay_asset_references.values().flatten())
    }
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
        self.initial_component_template(component)
            .map(AuthoringComponentTemplate::to_record)
    }

    pub(crate) fn initial_component_template(
        &self,
        component: &ComponentTypeId,
    ) -> Option<AuthoringComponentTemplate<'_>> {
        let entry = self.basis.schemas.get(component)?;
        if entry.adapter.is_none() || !component_allows_composition(&entry.schema) {
            return None;
        }
        Some(AuthoringComponentTemplate {
            schema: &entry.schema,
            value: entry.schema.authoring_initial_value()?,
            owner: &entry.owner,
        })
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
    ) -> Option<&ComponentSchema> {
        let fact = self.direct_record_facts.get(entity)?.get(component)?;
        let fact = &self.record_facts[*fact];
        if !fact.readiness.is_available() {
            return None;
        }
        self.basis
            .schemas
            .get(component)
            .map(|entry| &entry.schema)
            .filter(|schema| schema.version() == fact.version)
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

#[derive(Debug)]
pub(crate) struct PreparedSceneAuthoringPatch {
    pub(crate) document: SceneDocument,
    pub(crate) state: AuthoringDocumentState,
    pub(crate) patch: ScenePatchDocument,
    pub(crate) report: ScenePatchReport,
}

#[derive(Debug)]
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
    /// Admits a Scene against a newly constructed schema and Adapter basis.
    pub fn admit_authoring(
        self,
        source: AuthoringDocumentSource,
        bundle: &AuthoringSchemaBundle,
        adapters: &[AuthoringAdapterReceipt],
    ) -> Result<SceneAuthoringDocumentCandidate, AuthoringDocumentPublicationError> {
        let basis = AuthoringSchemaBasis::admit(bundle, adapters)?;
        self.admit_authoring_with_basis(source, &basis)
    }

    /// Admits an independent Scene source while retaining the supplied basis identity.
    ///
    /// Candidate decoding limits and complete document classification remain source-local.
    pub fn admit_authoring_with_basis(
        self,
        source: AuthoringDocumentSource,
        basis: &AuthoringSchemaBasis,
    ) -> Result<SceneAuthoringDocumentCandidate, AuthoringDocumentPublicationError> {
        let (document, limits) = self.into_authoring_parts();
        let state = classify_scene_document(
            &document,
            source,
            AuthoringDocumentKind::Scene,
            Arc::clone(&basis.0),
            limits,
        )?;
        Ok(SceneAuthoringDocumentCandidate { document, state })
    }
}

impl PrefabDocumentCandidate {
    /// Admits a Prefab against a newly constructed schema and Adapter basis.
    pub fn admit_authoring(
        self,
        source: AuthoringDocumentSource,
        bundle: &AuthoringSchemaBundle,
        adapters: &[AuthoringAdapterReceipt],
    ) -> Result<PrefabAuthoringDocumentCandidate, AuthoringDocumentPublicationError> {
        let basis = AuthoringSchemaBasis::admit(bundle, adapters)?;
        self.admit_authoring_with_basis(source, &basis)
    }

    /// Admits an independent Prefab source while retaining the supplied basis identity.
    ///
    /// Candidate decoding limits and complete document classification remain source-local.
    pub fn admit_authoring_with_basis(
        self,
        source: AuthoringDocumentSource,
        basis: &AuthoringSchemaBasis,
    ) -> Result<PrefabAuthoringDocumentCandidate, AuthoringDocumentPublicationError> {
        let (document, limits) = self.into_authoring_parts();
        let state = classify_scene_document(
            &document.instantiate(),
            source,
            AuthoringDocumentKind::Prefab,
            Arc::clone(&basis.0),
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
    ) -> Result<ClassifiedRecord<'_>, AuthoringDocumentPublicationError> {
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
        self.classify(component, record.version, owner, entry, true)
    }

    fn classify_overlay_record(
        &self,
        component: &ComponentTypeId,
        record: &SceneComponentRecord,
    ) -> Result<ClassifiedRecord<'_>, AuthoringDocumentPublicationError> {
        let owner = record
            .owner()
            .map_or(AuthoringRecordOwnerClaim::Unclaimed, |owner| {
                AuthoringRecordOwnerClaim::Declared(owner.clone())
            });
        let entry = self
            .schemas
            .get(component)
            .filter(|entry| record.owner().is_none_or(|owner| owner == &entry.owner));
        self.classify(component, record.version, owner, entry, false)
    }

    fn classify_overlay_target(
        &self,
        component: &ComponentTypeId,
        version: Option<ComponentSchemaVersion>,
    ) -> Result<ClassifiedRecord<'_>, AuthoringDocumentPublicationError> {
        let entry = self.schemas.get(component);
        self.classify(
            component,
            version
                .or_else(|| entry.map(|entry| entry.schema.version()))
                .unwrap_or(ComponentSchemaVersion::ONE),
            AuthoringRecordOwnerClaim::Unclaimed,
            entry,
            false,
        )
    }

    fn classify<'schema>(
        &'schema self,
        component: &ComponentTypeId,
        version: ComponentSchemaVersion,
        owner: AuthoringRecordOwnerClaim,
        entry: Option<&'schema AuthoringSchemaEntry>,
        require_scene_capability: bool,
    ) -> Result<ClassifiedRecord<'schema>, AuthoringDocumentPublicationError> {
        let Some(entry) = entry else {
            return Ok(ClassifiedRecord {
                owner,
                resolved_owner: None,
                readiness: AuthoringRecordReadiness::UnknownSchema,
                adapter: None,
                schema: None,
            });
        };
        if require_scene_capability && !entry.schema.has_capability(ComponentCapability::Scene) {
            return Err(rejection(with_public_locator(
                diagnostic_error(
                    "scene.authoring-component-not-scene-capable",
                    "Persistent Scene or Prefab record uses a component without Scene capability",
                ),
                "component-id",
                component.as_str(),
            )));
        }
        let readiness = if !entry.schema.has_capability(ComponentCapability::Scene) {
            AuthoringRecordReadiness::UnresolvedOverlay
        } else if version > entry.schema.version() {
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
            schema: Some(&entry.schema),
        })
    }
}

struct ClassifiedRecord<'schema> {
    owner: AuthoringRecordOwnerClaim,
    resolved_owner: Option<PersistentComponentSchemaOwnerId>,
    readiness: AuthoringRecordReadiness,
    adapter: Option<AuthoringAdapterEvidence>,
    schema: Option<&'schema ComponentSchema>,
}

struct ClassificationContext<'a, 'callback, 'environment> {
    source: &'a AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    basis: &'a AuthoringClassificationBasis,
    record_facts: Vec<AuthoringRecordFact>,
    overlay_asset_references: BTreeMap<SceneEntityId, Vec<DeclaredAssetReference>>,
    manifest: Vec<UnavailableRecordManifestEntry>,
    placements: BTreeMap<SceneEntityId, crate::ScenePlacement>,
    namespace: BTreeSet<&'a SceneEntityId>,
    limits: SceneFileLimits,
    // Selector scans share their own allowance, including unsuccessful lookups.
    property_read_budget: nara_reflect::ComponentPropertyReadBudget,
    asset_reference_nodes: usize,
    asset_reference_bytes: usize,
    admission: Option<&'callback mut reclassification::ClassificationCallback<'environment>>,
}

impl ClassificationContext<'_, '_, '_> {
    // Traversal and default materialization use the full per-record limits. Retained output
    // is reserved separately so an earlier record cannot shrink another record's Schema budget.
    fn asset_reference_limits(&self) -> nara_reflect::ComponentValueMaterializationLimits {
        self.limits.component_materialization_limits()
    }

    fn reserve_asset_references(
        &mut self,
        references: &[DeclaredAssetReference],
    ) -> Result<(), AuthoringDocumentPublicationError> {
        for reference in references {
            self.asset_reference_nodes = self.asset_reference_nodes.saturating_add(1);
            self.asset_reference_bytes = self
                .asset_reference_bytes
                .saturating_add(reference.field_id().as_str().len())
                .saturating_add(match reference.asset_ref() {
                    AssetRef::Path(path) => path.as_str().len(),
                    AssetRef::StableId(_) => 36,
                });
            for segment in reference.path().segments() {
                self.asset_reference_bytes =
                    self.asset_reference_bytes.saturating_add(match segment {
                        ComponentFieldPathSegment::Field(name) => name.len(),
                        ComponentFieldPathSegment::Index(_) => std::mem::size_of::<u32>(),
                    });
            }
        }
        if self.asset_reference_nodes > self.limits.component_value_nodes().get()
            || self.asset_reference_bytes > self.limits.component_value_bytes().get()
        {
            return Err(asset_reference_budget_rejection());
        }
        Ok(())
    }
}

fn asset_reference_budget_rejection() -> AuthoringDocumentPublicationError {
    rejection(diagnostic_error(
        "scene.authoring-asset-reference-budget-exceeded",
        "Persistent authoring asset reference facts exceed their aggregate budget",
    ))
}

fn classify_scene_document(
    document: &SceneDocument,
    source: AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    basis: Arc<AuthoringClassificationBasis>,
    limits: SceneFileLimits,
) -> Result<AuthoringDocumentState, AuthoringDocumentPublicationError> {
    classify_scene_document_impl(document, source, kind, basis, limits, None)
}

fn classify_scene_document_impl(
    document: &SceneDocument,
    source: AuthoringDocumentSource,
    kind: AuthoringDocumentKind,
    basis: Arc<AuthoringClassificationBasis>,
    limits: SceneFileLimits,
    mut admission: Option<&mut reclassification::ClassificationCallback<'_>>,
) -> Result<AuthoringDocumentState, AuthoringDocumentPublicationError> {
    reclassification::charge(&mut admission, "classification-structure", || {
        reclassification::structure_bytes(document)
    })?;
    let structure = validate_scene_structure(document);
    if structure.has_errors() {
        return Err(AuthoringDocumentPublicationError::new(structure));
    }

    let (record_facts, overlay_asset_references, manifest) = {
        let mut context = ClassificationContext {
            source: &source,
            kind,
            basis: &basis,
            record_facts: Vec::new(),
            overlay_asset_references: BTreeMap::new(),
            manifest: Vec::new(),
            placements: crate::patch::authoring_entity_placements(
                &document.roots,
                &document.entities,
            ),
            namespace: document.entities.iter().map(|entity| &entity.id).collect(),
            limits,
            property_read_budget: nara_reflect::ComponentPropertyReadBudget::new(
                limits.component_value_nodes(),
                limits.component_value_bytes(),
            ),
            asset_reference_nodes: 0,
            asset_reference_bytes: 0,
            admission: admission.as_deref_mut(),
        };
        for entity in &document.entities {
            classify_entity_components(entity, &mut context)?;
        }
        (
            context.record_facts,
            context.overlay_asset_references,
            context.manifest,
        )
    };
    reclassification::charge_measured(&mut admission, "classification-overlays", |scratch| {
        reclassification::overlay_copies_bytes(document, limits, scratch)
    })?;
    let unavailable_manifest = UnavailableRecordManifest::new(
        manifest,
        document
            .entities
            .iter()
            .filter_map(|entity| {
                entity
                    .prefab
                    .as_ref()
                    .map(|instance| (entity.id.clone(), instance.clone()))
            })
            .collect(),
    );
    reclassification::charge(&mut admission, "classification-diagnostics", || {
        reclassification::diagnostics_bytes(unavailable_manifest.entries())
    })?;
    let diagnostics = unavailable_record_diagnostics(unavailable_manifest.entries());
    let readiness = if unavailable_manifest.total_count() == 0 {
        AuthoringDocumentReadiness::Complete
    } else {
        AuthoringDocumentReadiness::Degraded
    };
    let mut direct_record_facts = BTreeMap::<_, BTreeMap<_, _>>::new();
    reclassification::charge(&mut admission, "classification-direct-index", || {
        reclassification::direct_index_bytes(&record_facts)
    })?;
    for (index, fact) in record_facts.iter().enumerate() {
        if let AuthoringRecordLocation::EntityComponent { entity } = &fact.location {
            direct_record_facts
                .entry(entity.clone())
                .or_default()
                .insert(fact.component.clone(), index);
        }
    }
    Ok(AuthoringDocumentState {
        source,
        kind,
        readiness,
        record_facts,
        overlay_asset_references,
        direct_record_facts,
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
            UnavailableRecordManifestEntry::OverlayTarget(target) => {
                let diagnostic = unavailable_readiness_diagnostic(target.readiness());
                let diagnostic = with_asset_ref(
                    with_public_locator(diagnostic, "component-id", target.component().as_str()),
                    "source",
                    target.source().as_asset_ref(),
                );
                diagnostics.push(with_authoring_record_location(
                    diagnostic,
                    target.location(),
                ));
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
        AuthoringRecordReadiness::UnresolvedOverlay => diagnostic_warning(
            "scene.authoring-unresolved-overlay",
            "Persistent overlay intent is retained for repair against its declared Schema",
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
        AuthoringRecordLocation::OverlayComponent {
            anchor,
            overlays,
            slot,
            target,
            ..
        } => {
            let diagnostic = with_public_locator(
                with_public_locator(diagnostic, "anchor-id", anchor.as_str()),
                "entity-id",
                target.entity.entity().as_str(),
            );
            let diagnostic = crate::diagnostics::with_public_u64(
                diagnostic,
                "overlay-depth",
                overlays.len() as u64,
            );
            let (kind, index) = match slot {
                AuthoringOverlayRecordSlot::Addition { index } => ("addition", *index),
                AuthoringOverlayRecordSlot::Component { index } => ("component", *index),
                AuthoringOverlayRecordSlot::Property { claim_index, .. } => {
                    ("property", *claim_index)
                }
            };
            crate::diagnostics::with_public_u64(
                with_public_locator(diagnostic, "overlay-declaration", kind),
                "declaration-index",
                index as u64,
            )
        }
    }
}

fn classify_entity_components(
    entity: &SceneEntityRecord,
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    reclassification::charge(&mut context.admission, "entity-placement-copy", || {
        retention::placement_dynamic_bytes(context.placements.get(&entity.id))
    })?;
    let placement = context.placements.get(&entity.id).cloned();
    for (component, record) in &entity.components {
        reclassification::charge(&mut context.admission, "record-location", || {
            crate::provenance::provenance_string_bytes(entity.id.as_str()).and_then(|bytes| {
                bytes.checked_add(retention::placement_dynamic_bytes(placement.as_ref())?)
            })
        })?;
        let location = AuthoringRecordLocation::EntityComponent {
            entity: entity.id.clone(),
        };
        classify_complete_record(location, placement.clone(), component, record, context)?;
    }
    if let Some(prefab) = &entity.prefab {
        overlay::classify_instance(prefab, &entity.id, context)?;
    }
    Ok(())
}

fn classify_complete_record(
    location: AuthoringRecordLocation,
    placement: Option<crate::ScenePlacement>,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    reclassification::charge(&mut context.admission, "record-classification", || {
        reclassification::record_identity_bytes(context.basis, component, record)
    })?;
    let classified = context.basis.classify_record(component, record)?;
    let bounded = context.admission.is_some();
    let mut admitted_effective = None;
    if classified.readiness.is_available() {
        let schema = classified
            .schema
            .as_ref()
            .expect("available authoring record retains its current schema");
        let owner = classified
            .resolved_owner
            .as_ref()
            .expect("available authoring record retains its admitted schema owner");
        let effective = if bounded {
            reclassification::complete_value(
                schema,
                owner,
                component,
                &record.value,
                context.limits,
                &mut context.admission,
            )?
        } else {
            validate_complete_value(schema, owner, component, &record.value, context.limits)?
        };
        if matches!(&location, AuthoringRecordLocation::EntityComponent { .. }) {
            if bounded {
                reclassification::validate_local_references(
                    schema,
                    &effective,
                    &context.namespace,
                    reference_traversal_limits(context.limits.component_materialization_limits()),
                    &mut context.admission,
                )?;
            } else {
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
        if bounded {
            admitted_effective = Some(effective);
        }
    }
    let schema_declared_asset_references = if classified
        .schema
        .as_ref()
        .is_none_or(|schema| record.version != schema.version())
    {
        Vec::new()
    } else {
        let schema = classified
            .schema
            .as_ref()
            .expect("a current known record retains its schema");
        let has_assets = schema.fields().iter().any(|field| {
            field
                .shape()
                .contains_kind(nara_reflect::ComponentValueKind::AssetRef)
        });
        let reference_limits = if has_assets {
            context.asset_reference_limits()
        } else {
            context.limits.component_materialization_limits()
        };
        if bounded {
            let effective = match admitted_effective {
                Some(value) => value,
                None => reclassification::effective_value(
                    schema,
                    &record.value,
                    context.limits,
                    &mut context.admission,
                )?,
            };
            reclassification::asset_references(
                schema,
                record.version,
                &effective,
                if has_assets {
                    nara_reflect::EntityReferenceTraversalLimits::new(
                        reference_limits.nodes(),
                        reference_limits.bytes(),
                        reference_limits.depth(),
                    )
                } else {
                    reference_traversal_limits(reference_limits)
                },
                &mut context.admission,
            )?
        } else {
            collect_declared_asset_references_from_schema_with_limits(
            schema,
            record.version,
            &record.value,
            reference_limits,
            if has_assets {
                nara_reflect::EntityReferenceTraversalLimits::new(
                    reference_limits.nodes(),
                    reference_limits.bytes(),
                    reference_limits.depth(),
                )
            } else {
                reference_traversal_limits(reference_limits)
            },
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
        }
    };
    context.reserve_asset_references(&schema_declared_asset_references)?;
    retain_complete_record(
        location,
        placement,
        component,
        record,
        classified,
        schema_declared_asset_references,
        context,
    )?;
    Ok(())
}

fn retain_complete_record(
    location: AuthoringRecordLocation,
    placement: Option<crate::ScenePlacement>,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    classified: ClassifiedRecord<'_>,
    schema_declared_asset_references: Vec<DeclaredAssetReference>,
    context: &mut ClassificationContext<'_, '_, '_>,
) -> Result<(), AuthoringDocumentPublicationError> {
    reclassification::charge(&mut context.admission, "record-fact", || {
        reclassification::fact_copy_bytes(
            &location,
            component,
            &classified,
            &schema_declared_asset_references,
        )
    })?;
    reclassification::reserve_vec(
        &mut context.record_facts,
        1,
        &mut context.admission,
        "record-fact-vector",
    )?;
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
        reclassification::charge_measured(
            &mut context.admission,
            "unavailable-record-copy",
            |scratch| {
                reclassification::unavailable_copy_bytes(
                    context.source,
                    &location,
                    placement.as_ref(),
                    &context.placements,
                    component,
                    record,
                    context.limits,
                    scratch,
                )
            },
        )?;
        reclassification::reserve_vec(
            &mut context.manifest,
            1,
            &mut context.admission,
            "unavailable-manifest-vector",
        )?;
        let entity_references = if context.admission.is_some() {
            let mut references = Vec::new();
            collect_entity_references_at(
                &record.value,
                &mut Vec::new(),
                &mut references,
                &mut context.admission,
            )?;
            references
        } else {
            collect_entity_references(&record.value)
        };
        context
            .manifest
            .push(UnavailableRecordManifestEntry::Record(UnavailableRecord {
                source: context.source.clone(),
                document_kind: context.kind,
                placement,
                anchor_placement: match &location {
                    AuthoringRecordLocation::EntityComponent { .. } => None,
                    AuthoringRecordLocation::OverlayComponent { anchor, .. } => {
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
    collect_entity_references_at(value, &mut Vec::new(), &mut references, &mut None)
        .expect("classification without admission cannot refuse reference copies");
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
    admission: &mut Option<&mut reclassification::ClassificationCallback<'_>>,
) -> Result<(), AuthoringDocumentPublicationError> {
    match value {
        ComponentValue::EntityReference(reference) => {
            reclassification::charge(admission, "unavailable-reference", || {
                let mut bytes = path
                    .len()
                    .checked_mul(std::mem::size_of::<UnavailableValuePathSegment>())?;
                for segment in path.iter() {
                    if let BorrowedUnavailableValuePathSegment::Field(field) = segment {
                        bytes = bytes
                            .checked_add(crate::provenance::provenance_string_bytes(field)?)?;
                    }
                }
                bytes.checked_add(match reference {
                    EntityReference::SceneLocal { entity } => {
                        crate::provenance::provenance_string_bytes(entity.as_str())?
                    }
                    EntityReference::Persistent { entity } => {
                        crate::provenance::provenance_string_bytes(entity.namespace.as_str())?
                    }
                })
            })?;
            reclassification::reserve_vec(
                references,
                1,
                admission,
                "unavailable-reference-vector",
            )?;
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
                reclassification::reserve_vec(path, 1, admission, "unavailable-reference-path")?;
                path.push(BorrowedUnavailableValuePathSegment::Index(index));
                collect_entity_references_at(value, path, references, admission)?;
                let _ = path.pop();
            }
        }
        ComponentValue::Map(values) => {
            for (field, value) in values {
                reclassification::reserve_vec(path, 1, admission, "unavailable-reference-path")?;
                path.push(BorrowedUnavailableValuePathSegment::Field(field));
                collect_entity_references_at(value, path, references, admission)?;
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
    prepare_scene_authoring_patch_impl(document, state, patch, history, None)
}

pub(crate) fn prepare_scene_authoring_patch_with_admission(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
    admit: &mut reclassification::AdmissionCallback<'_>,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    prepare_scene_authoring_patch_impl(document, state, patch, history, Some(admit))
}

fn prepare_scene_authoring_patch_impl(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
    mut admit: Option<&mut reclassification::AdmissionCallback<'_>>,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    if patch.operations.len() > MAX_AUTHORING_EDIT_OPERATIONS {
        return Err(authoring_structure_rejection(
            "Authoring transaction has too many operations",
        ));
    }
    if patch
        .operations
        .iter()
        .any(|operation| !is_authoring_field_operation(operation))
    {
        return structure::prepare_scene_authoring_structure_with_admission(
            document, state, patch, history, admit,
        );
    }
    if let Some(admit) = admit.as_deref_mut() {
        reclassification::admit_document(&document.roots, &document.entities, state.limits, admit)?;
        reclassification::admit_patch_copy(patch, "field-edits", admit)?;
        reclassification::admit_patch_copy(patch, "forward-patch", admit)?;
        reclassification::admit_patch_copy(patch, "field-candidate-writes", admit)?;
    }
    let edits = field_edits_from_patch(patch)?;
    let edit = &edits[0];
    let candidate = field_candidate::prepare(document.clone(), state, &edits)?;
    let shape = if let Some(admit) = admit.as_deref_mut() {
        crate::format::validate_scene_publication_shape_with_admission(
            candidate.document(),
            state.limits,
            |bytes| admit("publication-buffer", Some(bytes)),
        )
        .map_err(|error| match error {
            crate::format::PublicationShapeAdmissionError::Admission(report) => report,
            crate::format::PublicationShapeAdmissionError::Format(_) => field_edit_rejection(
                edit,
                "scene.authoring-field-edit-format-invalid",
                "Authoring field edit exceeds the persistent Scene format boundary",
            ),
        })
    } else {
        crate::format::validate_scene_publication_shape(candidate.document(), state.limits).map_err(
            |_| {
                field_edit_rejection(
                    edit,
                    "scene.authoring-field-edit-format-invalid",
                    "Authoring field edit exceeds the persistent Scene format boundary",
                )
            },
        )
    };
    shape?;
    let candidate_state = if let Some(admit) = admit.as_deref_mut() {
        candidate.classify_with_admission(admit)?
    } else {
        candidate
            .classify()
            .map_err(reclassification::publication_report)?
    };
    if candidate_state.unavailable_manifest != state.unavailable_manifest {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-unavailable-manifest-changed",
            "Authoring field edit changed the unavailable-record manifest",
        ));
    }
    let (candidate, inverse) = candidate.into_parts();
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

pub(crate) fn prepare_scene_prefab_conversion_patch(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    footprint: &crate::authoring::prefab_conversion::PrefabConversionFootprint,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    structure::prepare_conversion_structure(document, state, patch, footprint)
}

pub(crate) fn prepare_scene_prefab_conversion_patch_with_admission(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    footprint: &crate::authoring::prefab_conversion::PrefabConversionFootprint,
    admit: &mut reclassification::AdmissionCallback<'_>,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    structure::prepare_conversion_structure_with_admission(document, state, patch, footprint, admit)
}

pub(crate) fn prepare_prefab_conversion_patch_with_admission(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    footprint: &crate::authoring::prefab_conversion::PrefabConversionFootprint,
    admit: &mut reclassification::AdmissionCallback<'_>,
) -> Result<PreparedPrefabAuthoringPatch, ScenePatchReport> {
    reclassification::admit_document(&document.roots, &document.entities, state.limits, admit)?;
    let prepared = structure::prepare_conversion_structure_with_admission(
        &document.instantiate(),
        state,
        patch,
        footprint,
        admit,
    )?;
    let document = PrefabDocument::new(prepared.document.roots, prepared.document.entities);
    reclassification::validate_prefab_shape(&document, state.limits, Some(admit))?;
    Ok(PreparedPrefabAuthoringPatch {
        document,
        state: prepared.state,
        patch: prepared.patch,
        report: prepared.report,
    })
}

pub(crate) fn prepare_scene_prefab_conversion_patch_with_work(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    footprint: &crate::authoring::prefab_conversion::PrefabConversionFootprint,
    work: &mut crate::authoring::prefab_conversion::work::ConversionWork,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    structure::prepare_conversion_structure_with_work(
        &document.roots,
        &document.entities,
        state,
        patch,
        footprint,
        work,
    )
}

pub(crate) fn prepare_prefab_conversion_patch_with_work(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    footprint: &crate::authoring::prefab_conversion::PrefabConversionFootprint,
    work: &mut crate::authoring::prefab_conversion::work::ConversionWork,
) -> Result<PreparedPrefabAuthoringPatch, ScenePatchReport> {
    let prepared = structure::prepare_conversion_structure_with_work(
        &document.roots,
        &document.entities,
        state,
        patch,
        footprint,
        work,
    )?;
    let document = PrefabDocument::new(prepared.document.roots, prepared.document.entities);
    crate::format::validate_prefab_publication_shape_with_admission(
        &document,
        state.limits,
        |bytes| work.reserve("publication-buffer", Some(bytes)),
    )
    .map_err(|error| match error {
        crate::format::PublicationShapeAdmissionError::Admission(report) => report,
        crate::format::PublicationShapeAdmissionError::Format(_) => authoring_structure_rejection(
            "Conversion exceeds the persistent Prefab format boundary",
        ),
    })?;
    Ok(PreparedPrefabAuthoringPatch {
        document,
        state: prepared.state,
        patch: prepared.patch,
        report: prepared.report,
    })
}

pub(crate) fn prepare_prefab_conversion_patch(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    footprint: &crate::authoring::prefab_conversion::PrefabConversionFootprint,
) -> Result<PreparedPrefabAuthoringPatch, ScenePatchReport> {
    let prepared =
        structure::prepare_conversion_structure(&document.instantiate(), state, patch, footprint)?;
    let document = PrefabDocument::new(prepared.document.roots, prepared.document.entities);
    if crate::format::validate_prefab_publication_shape(&document, state.limits).is_err() {
        return Err(authoring_structure_rejection(
            "Conversion exceeds the persistent Prefab format boundary",
        ));
    }
    Ok(PreparedPrefabAuthoringPatch {
        document,
        state: prepared.state,
        patch: prepared.patch,
        report: prepared.report,
    })
}

pub(crate) fn prepare_prefab_authoring_patch(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
) -> Result<PreparedPrefabAuthoringPatch, ScenePatchReport> {
    prepare_prefab_authoring_patch_impl(document, state, patch, history, None)
}

pub(crate) fn prepare_prefab_authoring_patch_with_admission(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
    admit: &mut reclassification::AdmissionCallback<'_>,
) -> Result<PreparedPrefabAuthoringPatch, ScenePatchReport> {
    prepare_prefab_authoring_patch_impl(document, state, patch, history, Some(admit))
}

fn prepare_prefab_authoring_patch_impl(
    document: &PrefabDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
    mut admit: Option<&mut reclassification::AdmissionCallback<'_>>,
) -> Result<PreparedPrefabAuthoringPatch, ScenePatchReport> {
    if patch.operations.len() > MAX_AUTHORING_EDIT_OPERATIONS {
        return Err(authoring_structure_rejection(
            "Authoring transaction has too many operations",
        ));
    }
    if patch
        .operations
        .iter()
        .any(|operation| !is_authoring_field_operation(operation))
    {
        if let Some(admit) = admit.as_deref_mut() {
            reclassification::admit_document(
                &document.roots,
                &document.entities,
                state.limits,
                admit,
            )?;
        }
        let prepared = structure::prepare_scene_authoring_structure_with_admission(
            &document.instantiate(),
            state,
            patch,
            history,
            admit.as_deref_mut(),
        )?;
        let candidate = PrefabDocument::new(prepared.document.roots, prepared.document.entities);
        reclassification::validate_prefab_shape(&candidate, state.limits, admit.as_deref_mut())?;
        return Ok(PreparedPrefabAuthoringPatch {
            document: candidate,
            state: prepared.state,
            patch: prepared.patch,
            report: prepared.report,
        });
    }
    if let Some(admit) = admit.as_deref_mut() {
        reclassification::admit_document(&document.roots, &document.entities, state.limits, admit)?;
        reclassification::admit_patch_copy(patch, "field-edits", admit)?;
        reclassification::admit_patch_copy(patch, "forward-patch", admit)?;
        reclassification::admit_patch_copy(patch, "field-candidate-writes", admit)?;
    }
    let edits = field_edits_from_patch(patch)?;
    let edit = &edits[0];
    let candidate = field_candidate::prepare(document.instantiate(), state, &edits)?;
    let candidate = if let Some(admit) = admit.as_deref_mut() {
        candidate.validate_prefab_shape_with_admission(admit)?
    } else {
        candidate.validate_prefab_shape().map_err(|_| {
            field_edit_rejection(
                edit,
                "scene.authoring-field-edit-format-invalid",
                "Authoring field edit exceeds the persistent Prefab format boundary",
            )
        })?
    };
    let candidate_state = if let Some(admit) = admit.as_deref_mut() {
        candidate.classify_with_admission(admit)?
    } else {
        candidate
            .classify()
            .map_err(reclassification::publication_report)?
    };
    if candidate_state.unavailable_manifest != state.unavailable_manifest {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-unavailable-manifest-changed",
            "Authoring field edit changed the unavailable-record manifest",
        ));
    }
    let (candidate, inverse) = candidate.into_parts();
    Ok(PreparedPrefabAuthoringPatch {
        document: PrefabDocument {
            roots: candidate.roots,
            entities: candidate.entities,
        },
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

pub(crate) fn is_authoring_field_operation(operation: &ScenePatchOperation) -> bool {
    matches!(
        operation,
        ScenePatchOperation::SetField { .. }
            | ScenePatchOperation::SetAssetRefField { .. }
            | ScenePatchOperation::RemoveField { .. }
    )
}

fn field_edits_from_patch(
    patch: &ScenePatchDocument,
) -> Result<Vec<AuthoringFieldChange>, ScenePatchReport> {
    if patch.format_version != ScenePatchDocument::CURRENT_FORMAT_VERSION {
        return Err(authoring_field_patch_rejection());
    }
    field_edits_from_operations(patch.operations.iter())
}

fn field_edits_from_operations<'a>(
    operations: impl Iterator<Item = &'a ScenePatchOperation> + Clone,
) -> Result<Vec<AuthoringFieldChange>, ScenePatchReport> {
    parse_authoring_field_operations(operations, |entity, component, version, field, value| {
        AuthoringFieldChange {
            entity: entity.clone(),
            component: component.clone(),
            component_version: version,
            field: field.clone(),
            value: value.cloned(),
        }
    })
}

fn validate_authoring_field_operations<'a>(
    operations: impl Iterator<Item = &'a ScenePatchOperation> + Clone,
) -> Result<(), ScenePatchReport> {
    parse_authoring_field_operations(operations, |_, _, _, _, _| ()).map(|_| ())
}

fn parse_authoring_field_operations<'a, T>(
    operations: impl Iterator<Item = &'a ScenePatchOperation> + Clone,
    mut field_change: impl FnMut(
        &SceneEntityId,
        &ComponentTypeId,
        ComponentSchemaVersion,
        &ComponentFieldId,
        Option<&ComponentValue>,
    ) -> T,
) -> Result<Vec<T>, ScenePatchReport> {
    let count = operations.clone().count();
    if count == 0 || count > MAX_AUTHORING_FIELD_WRITES {
        return Err(authoring_field_patch_rejection());
    }
    let mut fields = BTreeSet::new();
    let mut value_nodes = 0usize;
    let mut logical_bytes = 0usize;
    operations
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
                let cost = value
                    .cost_up_to(
                        MAX_AUTHORING_FIELD_VALUE_NODES.saturating_sub(value_nodes),
                        MAX_AUTHORING_FIELD_LOGICAL_BYTES.saturating_sub(logical_bytes),
                    )
                    .ok_or_else(authoring_field_patch_rejection)?;
                value_nodes = value_nodes.saturating_add(cost.nodes());
                logical_bytes = logical_bytes.saturating_add(cost.logical_bytes());
            }
            if !fields.insert((entity, component, field))
                || value_nodes > MAX_AUTHORING_FIELD_VALUE_NODES
                || logical_bytes > MAX_AUTHORING_FIELD_LOGICAL_BYTES
            {
                return Err(authoring_field_patch_rejection());
            }
            Ok(field_change(
                entity,
                component,
                *component_version,
                field,
                value,
            ))
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
    candidate: SceneDocument,
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
    prepare_admitted_field_edit(candidate, state, edit)
}

fn prepare_admitted_field_edit(
    mut candidate: SceneDocument,
    state: &AuthoringDocumentState,
    edit: &AuthoringFieldChange,
) -> Result<(SceneDocument, ScenePatchDocument), ScenePatchReport> {
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
            if field.has_capability(ComponentCapability::EntityRef) {
                "scene.authoring-entity-reference-edit-unavailable"
            } else {
                "scene.authoring-field-edit-not-preservation-safe"
            },
            if field.has_capability(ComponentCapability::EntityRef) {
                "Entity reference edits require source-local targets in the current document"
            } else {
                "Authoring field edit is outside the preservation-safe value whitelist"
            },
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
    if field.has_capability(ComponentCapability::EntityRef)
        && !authoring_field_change_is_preservation_safe(field, previous.as_ref())
    {
        return Err(field_edit_rejection(
            edit,
            "scene.authoring-entity-reference-edit-unavailable",
            "Entity reference edits require a source-local inverse value",
        ));
    }
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
        && supports_data_shape(field.shape())
        && match value {
            // The complete candidate is validated by classification before publication. Keep
            // value failures there so the shared validator retains its safe code and root field.
            Some(value) => {
                !field.has_capability(ComponentCapability::EntityRef)
                    || contains_only_source_local_entity_references(value)
            }
            None => {
                (!field.required || field.default_value.is_some())
                    && (!field.has_capability(ComponentCapability::EntityRef)
                        || field
                            .default_value
                            .as_ref()
                            .is_none_or(contains_only_source_local_entity_references))
                    && matches!(
                        field.path().segments().last(),
                        Some(ComponentFieldPathSegment::Field(_))
                    )
            }
        }
}

fn contains_only_source_local_entity_references(value: &ComponentValue) -> bool {
    match value {
        ComponentValue::EntityReference(EntityReference::SceneLocal { .. }) => true,
        ComponentValue::EntityReference(EntityReference::Persistent { .. }) => false,
        ComponentValue::List(values) => values
            .iter()
            .all(contains_only_source_local_entity_references),
        ComponentValue::Map(values) => values
            .values()
            .all(contains_only_source_local_entity_references),
        ComponentValue::Null
        | ComponentValue::Bool(_)
        | ComponentValue::I64(_)
        | ComponentValue::U64(_)
        | ComponentValue::F64(_)
        | ComponentValue::String(_) => true,
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
