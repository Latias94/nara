use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_core::{DocumentContentState, DocumentHistoryStatus, DocumentRevision};
use nara_diagnostic::DiagnosticReport;
use nara_fs::{
    FileIdentity, PublicationAtomicity, PublicationIdentityEvidence, ReplaceReceipt, StageStatus,
};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    AuthoringDocumentReadiness, AuthoringFieldEdit, AuthoringRecordFact, AuthoringSchemaBasis,
    PrefabAuthoringSession, PrefabDocument, PublishedAuthoringPrefabDocument,
    PublishedAuthoringSceneDocument, SceneAuthoringContentState, SceneAuthoringHistoryStatus,
    SceneAuthoringRevision, SceneAuthoringSession, SceneDocument, SceneEntityId,
    ScenePatchDocument, ScenePatchReport, UnavailableRecordManifest,
};

use crate::composition::{
    GenericCompositionDocument, PrefabCompositionCommand, PrefabCompositionOutcome,
    PrefabCompositionSelection, SceneCompositionCommand, SceneCompositionSelection,
    prepare_generic_composition, prepare_prefab_composition, prepare_scene_composition,
    scene_entity_components,
};
use crate::diagnostic;
use crate::entity_query::{
    EditorEntityCursor, EditorEntityPage, EditorEntityQuery, EditorEntityQueryPosition,
};
use crate::inspector::{SceneInspectorCommand, SceneInspectorCommandReport};
use crate::play::{SceneEditorModel, SceneEditorState};
use crate::snapshot::WorldIdentitySnapshot;
use crate::{EditorAtomicFieldEdit, EditorFieldTarget};
use crate::{
    EditorDocumentQueryBasis, EditorDocumentQueryRejection,
    document_query::EditorDocumentQueryOwner,
};

mod authoring_query;
mod construction;
mod extension;
mod prefab_conversion;
mod prefab_declarations;
mod prepared_edit;
mod schema_reclassification;
pub use construction::{CommittedEditorDocumentConstruction, PreparedEditorDocumentConstruction};
pub use extension::{
    EditorExtensionCommand, EditorExtensionDocument, EditorExtensionDocumentCandidate,
    EditorExtensionDocumentError, EditorExtensionDocumentView,
};
use extension::{ErasedExtensionDocument, TypedExtensionDocument};
pub use prefab_conversion::{
    CommittedEditorPrefabConversion, EditorPrefabConversionCommitError,
    EditorPrefabConversionRequest, PreparedEditorPrefabConversion,
};
pub use prepared_edit::PreparedEditorDocumentEdit;
pub use schema_reclassification::{
    EditorWorkspaceSchemaReclassificationCommit, EditorWorkspaceSchemaReclassificationRequest,
    PreparedEditorWorkspaceSchemaReclassification,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorDocumentId(u64);

impl EditorDocumentId {
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorDocumentKind {
    Scene,
    Prefab,
    Extension(&'static str),
}

/// Host-normalized source identity carried by a document slot.
///
/// The workspace does not parse or resolve this locator. The concrete Host owns normalization;
/// the workspace only preserves exact equality for persistence and stale-document checks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditorDocumentSourceBinding(Arc<str>);

impl EditorDocumentSourceBinding {
    #[doc(hidden)]
    #[must_use]
    pub fn new(normalized_locator: impl Into<Arc<str>>) -> Self {
        Self(normalized_locator.into())
    }

    #[must_use]
    pub fn locator(&self) -> &str {
        &self.0
    }
}

/// Canonical encoded content identity projected into UI-neutral editor state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EditorDocumentDigest {
    encoded_bytes: u64,
    hash: [u8; 32],
}

impl EditorDocumentDigest {
    #[must_use]
    pub const fn new(encoded_bytes: u64, hash: [u8; 32]) -> Self {
        Self {
            encoded_bytes,
            hash,
        }
    }

    #[must_use]
    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn hash(self) -> [u8; 32] {
        self.hash
    }
}

#[derive(Debug, Default)]
struct EditorWorkspacePersistenceIdentity;

/// Exclusive capability for advancing persistence state in one hosted workspace.
///
/// The capability is intentionally neither cloneable nor available from [`EditorWorkspace::new`].
/// A concrete Host obtains it together with the workspace, captures a linear checkpoint before
/// persistence, and may commit that checkpoint only after its own typed filesystem receipt has
/// been accepted.
#[derive(Debug)]
pub struct EditorPersistenceAuthority {
    identity: Arc<EditorWorkspacePersistenceIdentity>,
}

/// Linear, workspace-bound capture of one document revision awaiting persistence evidence.
#[derive(Debug)]
pub struct EditorPersistenceCheckpoint {
    identity: Arc<EditorWorkspacePersistenceIdentity>,
    document: EditorDocumentId,
    kind: EditorDocumentKind,
    source_binding: Option<EditorDocumentSourceBinding>,
    revision: SceneAuthoringRevision,
    content_state: SceneAuthoringContentState,
    content: EditorPersistenceContent,
}

#[derive(Debug)]
enum EditorPersistenceContent {
    Scene(SceneDocument),
    Prefab(PrefabDocument),
    Extension(Result<Vec<u8>, EditorExtensionDocumentError>),
}

/// Linear proof that one captured revision was published with the required filesystem evidence.
///
/// The value can only be created by consuming a [`ReplaceReceipt`] whose publication evidence
/// satisfies the editor save contract. It deliberately carries no public constructor or fields.
#[derive(Debug)]
pub struct EditorPersistenceCommit {
    checkpoint: EditorPersistenceCheckpoint,
    digest: EditorDocumentDigest,
}

impl EditorPersistenceCommit {
    /// Consumes accepted filesystem publication evidence for one captured editor revision.
    #[doc(hidden)]
    #[must_use]
    pub fn from_publication(
        checkpoint: EditorPersistenceCheckpoint,
        expected_previous: FileIdentity,
        receipt: ReplaceReceipt,
    ) -> Option<Self> {
        if receipt.previous_identity() != Some(expected_previous)
            || receipt.publication_atomicity() != PublicationAtomicity::AtomicNameSwitch
            || receipt.published_identity() != Some(receipt.candidate_identity())
            || !matches!(
                receipt.publication_identity_evidence(),
                PublicationIdentityEvidence::HandleBoundCandidate
                    | PublicationIdentityEvidence::PostPublishObserved
            )
            || receipt.durability().data_synced() != StageStatus::Achieved
            || receipt.durability().file_metadata_synced() != StageStatus::Achieved
            || receipt.durability().name_published() != StageStatus::Achieved
            || receipt.verified_content_digest().is_none()
        {
            return None;
        }

        let content_digest = receipt
            .verified_content_digest()
            .expect("matching published content evidence was checked above");
        Some(Self {
            checkpoint,
            digest: EditorDocumentDigest::new(content_digest.length(), *content_digest.as_bytes()),
        })
    }

    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.checkpoint.document
    }

    #[must_use]
    pub const fn revision(&self) -> DocumentRevision {
        self.checkpoint.revision
    }

    #[must_use]
    pub const fn digest(&self) -> EditorDocumentDigest {
        self.digest
    }
}

impl EditorPersistenceCheckpoint {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn kind(&self) -> EditorDocumentKind {
        self.kind
    }

    #[must_use]
    pub fn source_binding(&self) -> Option<&EditorDocumentSourceBinding> {
        self.source_binding.as_ref()
    }

    #[must_use]
    pub const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub const fn content_state(&self) -> SceneAuthoringContentState {
        self.content_state
    }

    #[doc(hidden)]
    #[must_use]
    pub fn scene(&self) -> Option<&SceneDocument> {
        match &self.content {
            EditorPersistenceContent::Scene(scene) => Some(scene),
            EditorPersistenceContent::Prefab(_) | EditorPersistenceContent::Extension(_) => None,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn prefab(&self) -> Option<&PrefabDocument> {
        match &self.content {
            EditorPersistenceContent::Prefab(prefab) => Some(prefab),
            EditorPersistenceContent::Scene(_) | EditorPersistenceContent::Extension(_) => None,
        }
    }

    /// Captured domain-encoded bytes. No mutable document or filesystem authority is exposed.
    #[must_use]
    pub fn extension_bytes(&self) -> Option<Result<&[u8], EditorExtensionDocumentError>> {
        match &self.content {
            EditorPersistenceContent::Extension(bytes) => {
                Some(bytes.as_deref().map_err(|error| *error))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceDocumentContext {
    active: Option<EditorDocumentId>,
    requested: Option<EditorDocumentId>,
    target: Option<EditorDocumentId>,
}

impl WorkspaceDocumentContext {
    #[must_use]
    fn new(active: Option<EditorDocumentId>, requested: Option<EditorDocumentId>) -> Self {
        Self {
            active,
            requested,
            target: requested.or(active),
        }
    }
}

#[derive(Debug, Default)]
pub struct EditorWorkspace {
    persistence_identity: Arc<EditorWorkspacePersistenceIdentity>,
    document_query_owner: EditorDocumentQueryOwner,
    next_document_id: u64,
    active_document: Option<EditorDocumentId>,
    documents: BTreeMap<EditorDocumentId, EditorDocumentSlot>,
}

#[derive(Debug)]
enum EditorDocumentSlot {
    Scene(EditorSceneSlot),
    Prefab(EditorPrefabSlot),
    Extension(EditorExtensionSlot),
}

struct EditorExtensionSlot {
    owner: Box<dyn ErasedExtensionDocument>,
    source_binding: EditorDocumentSourceBinding,
    revision: DocumentRevision,
    saved_revision: DocumentRevision,
    saved_content_state: DocumentContentState,
    saved_digest: Option<EditorDocumentDigest>,
    external_reload: EditorExternalReloadState,
    selection: EditorSelectionSet,
}

impl fmt::Debug for EditorExtensionSlot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorExtensionSlot")
            .field("type_id", &self.owner.type_id())
            .field("revision", &self.revision)
            .field("source_binding", &self.source_binding)
            .finish_non_exhaustive()
    }
}

impl EditorDocumentSlot {
    fn kind(&self) -> EditorDocumentKind {
        match self {
            Self::Scene(_) => EditorDocumentKind::Scene,
            Self::Prefab(_) => EditorDocumentKind::Prefab,
            Self::Extension(slot) => EditorDocumentKind::Extension(slot.owner.type_id()),
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Scene(slot) => slot.title(),
            Self::Prefab(slot) => slot.title(),
            Self::Extension(slot) => slot.owner.title(),
        }
    }

    fn revision(&self) -> SceneAuthoringRevision {
        match self {
            Self::Scene(slot) => slot.revision(),
            Self::Prefab(slot) => slot.revision(),
            Self::Extension(slot) => slot.revision,
        }
    }

    fn saved_revision(&self) -> SceneAuthoringRevision {
        match self {
            Self::Scene(slot) => slot.saved_revision(),
            Self::Prefab(slot) => slot.saved_revision(),
            Self::Extension(slot) => slot.saved_revision,
        }
    }

    fn is_dirty(&self) -> bool {
        match self {
            Self::Scene(slot) => slot.is_dirty(),
            Self::Prefab(slot) => slot.is_dirty(),
            Self::Extension(slot) => slot.owner.content_state() != slot.saved_content_state,
        }
    }

    fn history_status(&self) -> DocumentHistoryStatus {
        match self {
            Self::Scene(slot) => slot.session.history_status(),
            Self::Prefab(slot) => slot.session.history_status(),
            Self::Extension(slot) => slot.owner.history_status(),
        }
    }

    fn source_binding(&self) -> Option<&EditorDocumentSourceBinding> {
        match self {
            Self::Scene(slot) => slot.source_binding.as_ref(),
            Self::Prefab(slot) => Some(&slot.source_binding),
            Self::Extension(slot) => Some(&slot.source_binding),
        }
    }

    fn external_reload(&self) -> EditorExternalReloadState {
        match self {
            Self::Scene(slot) => slot.external_reload(),
            Self::Prefab(slot) => slot.external_reload(),
            Self::Extension(slot) => slot.external_reload,
        }
    }

    fn authoring_readiness(&self) -> Option<AuthoringDocumentReadiness> {
        match self {
            Self::Scene(slot) => slot.authoring_readiness(),
            Self::Prefab(slot) => slot.authoring_readiness(),
            Self::Extension(_) => None,
        }
    }

    fn authoring_schema_basis(&self) -> Option<AuthoringSchemaBasis> {
        match self {
            Self::Scene(slot) => slot.session.authoring_schema_basis(),
            Self::Prefab(slot) => slot.session.authoring_schema_basis(),
            Self::Extension(_) => None,
        }
    }

    fn selection_mut(&mut self) -> &mut EditorSelectionSet {
        match self {
            Self::Scene(slot) => &mut slot.selection,
            Self::Prefab(slot) => &mut slot.selection,
            Self::Extension(slot) => &mut slot.selection,
        }
    }

    fn selection(&self) -> &EditorSelectionSet {
        match self {
            Self::Scene(slot) => &slot.selection,
            Self::Prefab(slot) => &slot.selection,
            Self::Extension(slot) => &slot.selection,
        }
    }

    fn has_entity(&self, entity: &SceneEntityId) -> bool {
        match self {
            Self::Scene(slot) => document_has_entity(slot.session.document(), entity),
            Self::Prefab(slot) => prefab_has_entity(slot.session.document(), entity),
            Self::Extension(_) => false,
        }
    }

    fn retain_existing_selection(&mut self) {
        match self {
            Self::Scene(slot) => slot.selection.retain_existing(slot.session.document()),
            Self::Prefab(slot) => slot
                .selection
                .retain_existing_prefab(slot.session.document()),
            Self::Extension(_) => {}
        }
    }

    fn mark_external_changed(&mut self) {
        let state = if self.is_dirty() {
            EditorExternalReloadState::Conflict
        } else {
            EditorExternalReloadState::Pending
        };
        match self {
            Self::Scene(slot) => slot.external_reload = state,
            Self::Prefab(slot) => slot.external_reload = state,
            Self::Extension(slot) => slot.external_reload = state,
        }
    }

    fn owns_content_state(&self, state: SceneAuthoringContentState) -> bool {
        match self {
            Self::Scene(slot) => slot.session().owns_content_state(state),
            Self::Prefab(slot) => slot.session().owns_content_state(state),
            Self::Extension(slot) => slot.owner.content_state().source_id() == state.source_id(),
        }
    }

    fn set_saved_digest(&mut self, digest: EditorDocumentDigest) {
        match self {
            Self::Scene(slot) => slot.saved_digest = Some(digest),
            Self::Prefab(slot) => slot.saved_digest = Some(digest),
            Self::Extension(slot) => slot.saved_digest = Some(digest),
        }
    }

    fn apply_persistence_checkpoint(
        &mut self,
        revision: SceneAuthoringRevision,
        content_state: SceneAuthoringContentState,
        digest: EditorDocumentDigest,
    ) {
        match self {
            Self::Scene(slot) => slot.apply_persistence_checkpoint(revision, content_state, digest),
            Self::Prefab(slot) => {
                slot.apply_persistence_checkpoint(revision, content_state, digest)
            }
            Self::Extension(slot) => {
                slot.saved_revision = revision;
                slot.saved_content_state = content_state;
                slot.saved_digest = Some(digest);
                if slot.external_reload != EditorExternalReloadState::Conflict {
                    slot.external_reload = EditorExternalReloadState::Clean;
                }
            }
        }
    }
}

impl EditorWorkspace {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a workspace together with its exclusive persistence capability.
    ///
    /// This is the Host embedding path. UI adapters and ordinary workspace command producers
    /// should use [`Self::new`], which cannot advance saved checkpoints.
    #[doc(hidden)]
    #[must_use]
    pub fn new_hosted() -> (Self, EditorPersistenceAuthority) {
        let workspace = Self::new();
        let authority = EditorPersistenceAuthority {
            identity: Arc::clone(&workspace.persistence_identity),
        };
        (workspace, authority)
    }

    #[must_use]
    pub fn active_document(&self) -> Option<EditorDocumentId> {
        self.active_document
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Returns the stable document identities currently owned by this workspace.
    ///
    /// This Host-only snapshot supports non-destructive whole-workspace close preparation. UI
    /// adapters should consume [`EditorWorkspaceModel`] instead.
    #[doc(hidden)]
    #[must_use]
    pub fn __document_ids(&self) -> Vec<EditorDocumentId> {
        self.documents.keys().copied().collect()
    }

    #[must_use]
    pub fn scene(&self, document: EditorDocumentId) -> Option<&EditorSceneSlot> {
        match self.documents.get(&document) {
            Some(EditorDocumentSlot::Scene(slot)) => Some(slot),
            Some(EditorDocumentSlot::Prefab(_) | EditorDocumentSlot::Extension(_)) | None => None,
        }
    }

    #[must_use]
    pub fn prefab(&self, document: EditorDocumentId) -> Option<&EditorPrefabSlot> {
        match self.documents.get(&document) {
            Some(EditorDocumentSlot::Prefab(slot)) => Some(slot),
            Some(EditorDocumentSlot::Scene(_) | EditorDocumentSlot::Extension(_)) | None => None,
        }
    }

    #[must_use]
    pub fn document_kind(&self, document: EditorDocumentId) -> Option<EditorDocumentKind> {
        self.documents.get(&document).map(EditorDocumentSlot::kind)
    }

    #[must_use]
    pub fn document_is_dirty(&self, document: EditorDocumentId) -> Option<bool> {
        self.documents
            .get(&document)
            .map(EditorDocumentSlot::is_dirty)
    }

    #[must_use]
    pub fn has_dirty_prefab(&self) -> bool {
        self.documents
            .values()
            .any(|slot| slot.kind() == EditorDocumentKind::Prefab && slot.is_dirty())
    }

    #[must_use]
    pub fn document_revision(&self, document: EditorDocumentId) -> Option<SceneAuthoringRevision> {
        self.documents
            .get(&document)
            .map(EditorDocumentSlot::revision)
    }

    /// Reads the owner's content identity without encoding or advancing its revision.
    ///
    /// Undo may restore this identity while the accepted revision continues to advance.
    #[must_use]
    pub fn document_content_state(
        &self,
        document: EditorDocumentId,
    ) -> Option<SceneAuthoringContentState> {
        self.documents.get(&document).map(|slot| match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.content_state(),
            EditorDocumentSlot::Prefab(slot) => slot.session.content_state(),
            EditorDocumentSlot::Extension(slot) => slot.owner.content_state(),
        })
    }

    /// Captures the exact current query facts for an open Scene or Prefab document.
    pub fn document_query_basis(
        &self,
        document: EditorDocumentId,
    ) -> Result<EditorDocumentQueryBasis, EditorDocumentQueryRejection> {
        let slot = self
            .documents
            .get(&document)
            .ok_or(EditorDocumentQueryRejection::DocumentNotOpen { document })?;
        let kind = slot.kind();
        if !matches!(kind, EditorDocumentKind::Scene | EditorDocumentKind::Prefab) {
            return Err(EditorDocumentQueryRejection::UnsupportedDocumentKind { kind });
        }
        Ok(EditorDocumentQueryBasis {
            document,
            revision: slot.revision(),
            schema: slot.authoring_schema_basis(),
        })
    }

    /// Enumerates a bounded page of direct source entity identities without changing selection,
    /// active document, history or dirty state, or constructing an Inspector.
    ///
    /// Admission checks workspace ownership, document/kind, source/revision, then exact Schema
    /// admission. Continuations are replayable and never silently rebase. Only the requested
    /// records are visited; no Prefab expansion, field query or Runtime projection is performed.
    pub fn query_document_entities(
        &self,
        query: EditorEntityQuery,
    ) -> Result<EditorEntityPage, EditorDocumentQueryRejection> {
        let (basis, expected_kind, next_record) = match query.position {
            EditorEntityQueryPosition::Start(basis) => (basis, None, 0),
            EditorEntityQueryPosition::Continue(cursor) => {
                if cursor.owner != self.document_query_owner {
                    return Err(EditorDocumentQueryRejection::WrongWorkspace);
                }
                (cursor.basis, Some(cursor.kind), cursor.next_record)
            }
        };
        let slot = self.resolve_document_query_slot(&basis, expected_kind)?;
        let kind = slot.kind();
        let records = match slot {
            EditorDocumentSlot::Scene(slot) => &slot.session.document().entities,
            EditorDocumentSlot::Prefab(slot) => &slot.session.document().entities,
            EditorDocumentSlot::Extension(_) => {
                return Err(EditorDocumentQueryRejection::UnsupportedDocumentKind { kind });
            }
        };
        let remaining = records
            .get(next_record..)
            .ok_or(EditorDocumentQueryRejection::InvalidContinuation)?;
        let scanned_records = remaining
            .len()
            .min(query.limits.max_scan_records())
            .min(query.limits.max_rows());
        let entities = remaining[..scanned_records]
            .iter()
            .map(|record| record.id.clone())
            .collect();
        let next_record = next_record + scanned_records;
        let next = (next_record < records.len()).then(|| EditorEntityCursor {
            owner: self.document_query_owner.clone(),
            basis: basis.clone(),
            kind,
            next_record,
        });
        Ok(EditorEntityPage {
            basis,
            entities,
            scanned_records,
            next,
        })
    }

    #[must_use]
    pub fn document_source_binding(
        &self,
        document: EditorDocumentId,
    ) -> Option<&EditorDocumentSourceBinding> {
        self.documents
            .get(&document)
            .and_then(EditorDocumentSlot::source_binding)
    }

    /// Opens one independently owned non-Scene document in the same workspace lifecycle.
    pub fn open_extension_document<D: EditorExtensionDocument>(
        &mut self,
        source_binding: EditorDocumentSourceBinding,
        document: D,
    ) -> Result<EditorDocumentId, EditorWorkspaceCommandReport> {
        let context = WorkspaceDocumentContext::new(self.active_document, None);
        if D::TYPE_ID.is_empty() {
            return Err(workspace_error_report(
                context,
                "tooling.extension-document-type-empty",
                "extension document type identity must not be empty",
            ));
        }
        if self
            .document_for_source_binding(EditorDocumentKind::Extension(D::TYPE_ID), &source_binding)
            .is_some()
        {
            return Err(workspace_error_report(
                context,
                "tooling.extension-document-source-already-open",
                "a document for this source is already open",
            ));
        }
        let revision = DocumentRevision::initial();
        let content_state = document.content_state();
        let id = self.allocate_document_id();
        self.documents.insert(
            id,
            EditorDocumentSlot::Extension(EditorExtensionSlot {
                owner: Box::new(TypedExtensionDocument::new(document)),
                source_binding,
                revision,
                saved_revision: revision,
                saved_content_state: content_state,
                saved_digest: None,
                external_reload: EditorExternalReloadState::Clean,
                selection: EditorSelectionSet::default(),
            }),
        );
        self.active_document = Some(id);
        Ok(id)
    }

    /// Applies a domain-typed command to the exact open document owner.
    pub fn apply_extension_command<D: EditorExtensionDocument>(
        &mut self,
        document: EditorDocumentId,
        base_revision: DocumentRevision,
        command: D::Command,
    ) -> EditorWorkspaceCommandReport {
        self.apply_extension_action(EditorExtensionCommand::new::<D>(
            document,
            base_revision,
            command,
        ))
    }

    /// Applies the typed payload delivered through the ordinary Editor action path.
    #[doc(hidden)]
    pub fn apply_extension_action(
        &mut self,
        action: EditorExtensionCommand,
    ) -> EditorWorkspaceCommandReport {
        let document = action.document;
        let base_revision = action.base_revision;
        let active = self.active_document;
        let context = WorkspaceDocumentContext::new(active, Some(document));
        let Some(document_slot @ EditorDocumentSlot::Extension(_)) =
            self.documents.get_mut(&document)
        else {
            return workspace_document_error_report(context, document);
        };
        let EditorDocumentSlot::Extension(slot) = document_slot else {
            unreachable!()
        };
        if slot.owner.rust_type_id() != action.owner {
            return workspace_error_report(
                context,
                "tooling.extension-document-owner-mismatch",
                "extension operation does not match the open document owner",
            );
        }
        if slot.revision != base_revision {
            return workspace_revision_error_report(
                context,
                document,
                base_revision,
                slot.revision,
            );
        }
        let result = slot.owner.apply(action.payload.as_ref());
        match result {
            Ok(()) => slot.revision = slot.revision.next(),
            Err(error) => {
                return extension_error_report(context, error);
            }
        }
        report_for_document(document, active, document_slot)
    }

    #[must_use]
    /// Finds a source within its document namespace. The Host must additionally reject aliases
    /// between namespaces when their configured source roots overlap.
    pub fn document_for_source_binding(
        &self,
        kind: EditorDocumentKind,
        source_binding: &EditorDocumentSourceBinding,
    ) -> Option<EditorDocumentId> {
        self.documents.iter().find_map(|(document, slot)| {
            // Scene and Prefab locators use their Host's source scope; extension locators share
            // one scope even across distinct document owners.
            let same_namespace = matches!(
                (slot.kind(), kind),
                (EditorDocumentKind::Scene, EditorDocumentKind::Scene)
                    | (EditorDocumentKind::Prefab, EditorDocumentKind::Prefab)
                    | (
                        EditorDocumentKind::Extension(_),
                        EditorDocumentKind::Extension(_)
                    )
            );
            (same_namespace && slot.source_binding() == Some(source_binding)).then_some(*document)
        })
    }

    pub fn activate_document(
        &mut self,
        document: EditorDocumentId,
    ) -> EditorWorkspaceCommandReport {
        let context = WorkspaceDocumentContext::new(self.active_document, Some(document));
        let Some(slot) = self.documents.get(&document) else {
            return workspace_document_error_report(context, document);
        };
        self.active_document = Some(document);
        report_for_document(document, self.active_document, slot)
    }

    #[must_use]
    pub fn active_scene(&self) -> Option<&EditorSceneSlot> {
        self.active_document.and_then(|id| self.scene(id))
    }

    #[must_use]
    pub fn active_prefab(&self) -> Option<&EditorPrefabSlot> {
        self.active_document.and_then(|id| self.prefab(id))
    }

    /// Checks that an Adapter-provided baseline exactly matches the accepted Prefab revision.
    #[doc(hidden)]
    #[must_use]
    pub fn __prefab_field_edit_matches(
        &self,
        registry: &ComponentRegistry,
        edit: &EditorAtomicFieldEdit,
    ) -> bool {
        edit.writes().iter().all(|write| {
            prefab_field_value(self, registry, edit.target(), write.field()) == Some(write.value())
        })
    }

    /// Checks whether an exact revision-bound Prefab field still exists.
    #[doc(hidden)]
    #[must_use]
    pub fn __prefab_field_target_exists(
        &self,
        registry: &ComponentRegistry,
        target: &EditorFieldTarget,
    ) -> bool {
        prefab_field_value(self, registry, target, target.field()).is_some()
    }

    fn apply_persistence_checkpoint(
        &mut self,
        document: EditorDocumentId,
        kind: EditorDocumentKind,
        source_binding: Option<&EditorDocumentSourceBinding>,
        revision: SceneAuthoringRevision,
        content_state: SceneAuthoringContentState,
        digest: EditorDocumentDigest,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(slot) = self.documents.get_mut(&document) else {
            return workspace_document_error_report(context, document);
        };
        let current = slot.revision();
        let saved = slot.saved_revision();
        let revision_matches_source = revision.source_id() == current.source_id();
        let revision_not_ahead = revision.generation() <= current.generation();
        let revision_not_regressed = revision.source_id() == saved.source_id()
            && revision.generation() >= saved.generation();
        if slot.kind() != kind
            || slot.source_binding() != source_binding
            || !revision_matches_source
            || !revision_not_ahead
            || !revision_not_regressed
            || !slot.owns_content_state(content_state)
        {
            return workspace_error_report(
                context,
                "tooling.workspace-persistence-checkpoint-mismatch",
                "persistence checkpoint does not match the open document revision",
            );
        }
        slot.apply_persistence_checkpoint(revision, content_state, digest);
        report_for_document(document, active_document, slot)
    }

    fn bind_opened_source_digest(
        &mut self,
        document: EditorDocumentId,
        digest: EditorDocumentDigest,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(slot) = self.documents.get_mut(&document) else {
            return workspace_document_error_report(context, document);
        };
        slot.set_saved_digest(digest);
        report_for_document(document, active_document, slot)
    }

    pub fn open_scene_session(
        &mut self,
        title: impl Into<String>,
        session: SceneAuthoringSession,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        self.open_scene_session_with_source_binding(title, None, session)
    }

    fn open_scene_session_with_source_binding(
        &mut self,
        title: impl Into<String>,
        source_binding: Option<EditorDocumentSourceBinding>,
        session: SceneAuthoringSession,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        if session.live_instance().is_some() {
            let context = WorkspaceDocumentContext {
                active: self.active_document,
                requested: None,
                target: None,
            };
            return Err(EditorSceneSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-session-attached",
                    "workspace publication requires a detached scene authoring session",
                ),
                session,
            ));
        }

        let document_id = self.allocate_document_id();
        let slot = EditorSceneSlot::from_session(title, source_binding, session);
        self.documents
            .insert(document_id, EditorDocumentSlot::Scene(slot));
        self.active_document = Some(document_id);
        let dirty = self
            .documents
            .get(&document_id)
            .is_some_and(EditorDocumentSlot::is_dirty);
        Ok(EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document_id),
            opened_document: Some(document_id),
            active_document: self.active_document,
            dirty: Some(dirty),
            revision: self
                .documents
                .get(&document_id)
                .map(EditorDocumentSlot::revision),
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        })
    }

    /// Publishes one structurally admitted Scene document without introducing a second owner for
    /// document, history, or selection state.
    pub fn open_authoring_scene_document(
        &mut self,
        title: impl Into<String>,
        source_binding: EditorDocumentSourceBinding,
        published: PublishedAuthoringSceneDocument,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        self.open_scene_session_with_source_binding(
            title,
            Some(source_binding),
            SceneAuthoringSession::from_published_authoring_document(published),
        )
    }

    pub fn open_prefab_session(
        &mut self,
        title: impl Into<String>,
        source_binding: EditorDocumentSourceBinding,
        session: PrefabAuthoringSession,
    ) -> Result<EditorWorkspaceCommandReport, EditorPrefabSessionPublicationError> {
        let document_id = self.allocate_document_id();
        let slot = EditorPrefabSlot::from_session(title, source_binding, session);
        self.documents
            .insert(document_id, EditorDocumentSlot::Prefab(slot));
        self.active_document = Some(document_id);
        let slot = self
            .documents
            .get(&document_id)
            .expect("newly opened Prefab slot must exist");
        Ok(EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document_id),
            opened_document: Some(document_id),
            active_document: self.active_document,
            dirty: Some(slot.is_dirty()),
            revision: Some(slot.revision()),
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        })
    }

    /// Publishes one structurally admitted Prefab document into the existing Prefab session slot.
    pub fn open_authoring_prefab_document(
        &mut self,
        title: impl Into<String>,
        source_binding: EditorDocumentSourceBinding,
        published: PublishedAuthoringPrefabDocument,
    ) -> Result<EditorWorkspaceCommandReport, EditorPrefabSessionPublicationError> {
        self.open_prefab_session(
            title,
            source_binding,
            PrefabAuthoringSession::from_published_authoring_document(published),
        )
    }

    pub fn apply_command(
        &mut self,
        registry: &ComponentRegistry,
        command: EditorWorkspaceCommand,
    ) -> EditorWorkspaceCommandReport {
        match command {
            EditorWorkspaceCommand::CloseDocument { document } => self.close_document(document),
            EditorWorkspaceCommand::SetActiveDocument { document } => {
                self.set_active_document(document)
            }
            EditorWorkspaceCommand::SelectEntity { document, entity } => {
                self.select_entity(document, entity)
            }
            EditorWorkspaceCommand::ApplyInspectorCommand {
                document,
                base_revision,
                command,
            } => self.apply_inspector_command(registry, document, base_revision, command),
            EditorWorkspaceCommand::ApplyScenePatch {
                document,
                base_revision,
                patch,
            } => self.apply_scene_patch(registry, document, base_revision, patch),
            EditorWorkspaceCommand::ApplyPrefabPatch {
                document,
                base_revision,
                patch,
            } => self.apply_prefab_patch(registry, document, base_revision, patch),
            EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                document,
                base_revision,
                edit,
            } => self.apply_authoring_field_edit(document, base_revision, edit),
            EditorWorkspaceCommand::ComposeScene {
                document,
                base_revision,
                command,
            } => self.apply_scene_composition(registry, document, base_revision, command),
            EditorWorkspaceCommand::ComposePrefab {
                document,
                base_revision,
                command,
            } => self.apply_prefab_composition(registry, document, base_revision, command),
            EditorWorkspaceCommand::Undo {
                document,
                base_revision,
            } => self.undo(registry, document, base_revision),
            EditorWorkspaceCommand::Redo {
                document,
                base_revision,
            } => self.redo(registry, document, base_revision),
            EditorWorkspaceCommand::MarkExternalChanged { document } => {
                self.mark_external_changed(document)
            }
        }
    }

    /// Applies the closed command vocabulary for generically published authoring documents.
    ///
    /// This path never constructs or consults a Runtime component registry. Bounded field edits
    /// and finite composition transactions preserve the complete unavailable-record manifest.
    pub fn apply_authoring_command(
        &mut self,
        command: EditorWorkspaceCommand,
    ) -> EditorWorkspaceCommandReport {
        match command {
            EditorWorkspaceCommand::CloseDocument { document } => {
                if let Err(report) = self.require_authoring_document(document) {
                    return report;
                }
                self.close_document(document)
            }
            EditorWorkspaceCommand::SetActiveDocument { document } => {
                if let Err(report) = self.require_authoring_document(document) {
                    return report;
                }
                self.set_active_document(document)
            }
            EditorWorkspaceCommand::SelectEntity { document, entity } => {
                if let Err(report) = self.require_authoring_document(document) {
                    return report;
                }
                self.select_entity(document, entity)
            }
            EditorWorkspaceCommand::ApplyInspectorCommand {
                document,
                base_revision,
                command,
            } => self.apply_authoring_inspector_command(document, base_revision, command),
            EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                document,
                base_revision,
                edit,
            } => self.apply_authoring_field_edit(document, base_revision, edit),
            EditorWorkspaceCommand::ApplyScenePatch {
                document,
                base_revision,
                patch,
            } => self.apply_authoring_patch(
                document,
                base_revision,
                EditorDocumentKind::Scene,
                patch,
            ),
            EditorWorkspaceCommand::ApplyPrefabPatch {
                document,
                base_revision,
                patch,
            } => self.apply_authoring_patch(
                document,
                base_revision,
                EditorDocumentKind::Prefab,
                patch,
            ),
            EditorWorkspaceCommand::Undo {
                document,
                base_revision,
            } => self.undo_authoring(document, base_revision),
            EditorWorkspaceCommand::Redo {
                document,
                base_revision,
            } => self.redo_authoring(document, base_revision),
            EditorWorkspaceCommand::MarkExternalChanged { document } => {
                if let Err(report) = self.require_authoring_document(document) {
                    return report;
                }
                self.mark_external_changed(document)
            }
            EditorWorkspaceCommand::ComposeScene {
                document,
                base_revision,
                command,
            } => self.apply_generic_composition(
                document,
                base_revision,
                EditorDocumentKind::Scene,
                command,
            ),
            EditorWorkspaceCommand::ComposePrefab {
                document,
                base_revision,
                command,
            } => {
                let command = match command {
                    PrefabCompositionCommand::CreateEntity { draft, placement }
                        if draft.components().is_empty() =>
                    {
                        SceneCompositionCommand::CreateEntity {
                            kind: crate::SceneCompositionEntityKind::Empty,
                            placement,
                        }
                    }
                    PrefabCompositionCommand::AddComponent { entity, component } => {
                        SceneCompositionCommand::AddComponent { entity, component }
                    }
                    PrefabCompositionCommand::RemoveComponent { entity, component } => {
                        SceneCompositionCommand::RemoveComponent { entity, component }
                    }
                    PrefabCompositionCommand::MoveEntity { entity, to } => {
                        SceneCompositionCommand::MoveEntity { entity, to }
                    }
                    PrefabCompositionCommand::RemoveEntity { entity } => {
                        SceneCompositionCommand::RemoveEntity { entity }
                    }
                    _ => return authoring_command_error_report(self.active_document, document),
                };
                self.apply_generic_composition(
                    document,
                    base_revision,
                    EditorDocumentKind::Prefab,
                    command,
                )
            }
        }
    }

    #[must_use]
    pub fn model(
        &self,
        registry: &ComponentRegistry,
        edit_world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> EditorWorkspaceModel {
        let active_scene =
            self.active_document
                .and_then(|document| match self.documents.get(&document)? {
                    EditorDocumentSlot::Scene(slot) => {
                        Some(slot.model(document, registry, edit_world_snapshot))
                    }
                    EditorDocumentSlot::Prefab(slot) => Some(slot.model(document, Some(registry))),
                    EditorDocumentSlot::Extension(_) => None,
                });

        self.model_with_active_scene(active_scene)
    }

    /// Projects generically published authoring documents without a Runtime registry.
    #[must_use]
    pub fn authoring_model(&self) -> EditorWorkspaceModel {
        let active_scene =
            self.active_document
                .and_then(|document| match self.documents.get(&document)? {
                    EditorDocumentSlot::Scene(slot) => Some(slot.authoring_model(document)),
                    EditorDocumentSlot::Prefab(slot) => Some(slot.model(document, None)),
                    EditorDocumentSlot::Extension(_) => None,
                });

        self.model_with_active_scene(active_scene)
    }

    fn model_with_active_scene(
        &self,
        active_scene: Option<EditorSceneModel>,
    ) -> EditorWorkspaceModel {
        EditorWorkspaceModel {
            active_document: self.active_document,
            documents: self
                .documents
                .iter()
                .map(|(document, slot)| EditorDocumentTabModel {
                    document: *document,
                    kind: slot.kind(),
                    title: slot.title().to_owned(),
                    dirty: slot.is_dirty(),
                    revision: slot.revision(),
                    saved_revision: slot.saved_revision(),
                    history: slot.history_status(),
                    authoring_readiness: slot.authoring_readiness(),
                    authoring_schema_basis: slot.authoring_schema_basis(),
                    source_binding: slot.source_binding().cloned(),
                    external_reload: slot.external_reload(),
                    active: Some(*document) == self.active_document,
                })
                .collect(),
            scenes: self
                .documents
                .iter()
                .filter_map(|(document, slot)| {
                    let EditorDocumentSlot::Scene(slot) = slot else {
                        return None;
                    };
                    Some(EditorSceneTabModel {
                        document: *document,
                        title: slot.title.clone(),
                        dirty: slot.is_dirty(),
                        revision: slot.revision(),
                        external_reload: slot.external_reload,
                        active: Some(*document) == self.active_document,
                    })
                })
                .collect(),
            active_scene,
            extension_documents: self
                .documents
                .iter()
                .filter_map(|(document, slot)| {
                    let EditorDocumentSlot::Extension(slot) = slot else {
                        return None;
                    };
                    Some(EditorExtensionDocumentView {
                        document: *document,
                        type_id: slot.owner.type_id(),
                        owner: slot.owner.rust_type_id(),
                        revision: slot.revision,
                        value: slot.owner.view(),
                    })
                })
                .collect(),
            diagnostics: DiagnosticReport::default(),
        }
    }

    fn close_document(&mut self, document: EditorDocumentId) -> EditorWorkspaceCommandReport {
        let context = WorkspaceDocumentContext::new(self.active_document, Some(document));
        let Some(slot) = self.documents.get(&document) else {
            return workspace_document_error_report(context, document);
        };
        if slot.is_dirty() {
            return workspace_error_report(
                context,
                "tooling.workspace-dirty-close-decision-required",
                "closing a dirty document requires an explicit save, discard, or cancel decision",
            );
        }
        self.remove_document(document)
    }

    /// Removes one dirty document after a concrete Host has recorded an explicit discard choice.
    ///
    /// This is Host plumbing rather than an author command. Product paths must present the
    /// Save/Discard/Cancel decision before invoking it.
    #[doc(hidden)]
    pub fn __discard_document(
        &mut self,
        document: EditorDocumentId,
    ) -> EditorWorkspaceCommandReport {
        let context = WorkspaceDocumentContext::new(self.active_document, Some(document));
        if !self.documents.contains_key(&document) {
            return workspace_document_error_report(context, document);
        }
        self.remove_document(document)
    }

    fn remove_document(&mut self, document: EditorDocumentId) -> EditorWorkspaceCommandReport {
        let removed = self.documents.remove(&document);
        debug_assert!(removed.is_some(), "the document was checked before removal");
        if self.active_document == Some(document) {
            self.active_document = self.documents.keys().next().copied();
        }
        EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document),
            active_document: self.active_document,
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        }
    }

    fn set_active_document(&mut self, document: EditorDocumentId) -> EditorWorkspaceCommandReport {
        self.activate_document(document)
    }

    fn select_entity(
        &mut self,
        document: EditorDocumentId,
        entity: Option<SceneEntityId>,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(slot) = self.documents.get_mut(&document) else {
            return workspace_document_error_report(context, document);
        };
        if let Some(entity) = &entity
            && !slot.has_entity(entity)
        {
            return workspace_entity_error_report(
                context,
                document,
                "tooling.workspace-missing-entity",
                "workspace selection targets an entity that is not in the document",
                entity,
            );
        }
        slot.selection_mut().select_entity(entity);
        report_for_document(document, active_document, slot)
    }

    fn apply_inspector_command(
        &mut self,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: SceneInspectorCommand,
    ) -> EditorWorkspaceCommandReport {
        if self
            .documents
            .get(&document)
            .is_some_and(|slot| slot.authoring_readiness().is_some())
        {
            return self.apply_authoring_inspector_command(document, base_revision, command);
        }
        let (active_document, slot) =
            match self.resolve_exact_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        match slot {
            EditorDocumentSlot::Scene(slot) => {
                let inspector_report =
                    slot.editor
                        .apply_inspector_command(&mut slot.session, registry, command);
                if inspector_report.applied {
                    slot.selection.retain_existing(slot.session.document());
                }
                let mut report = report_for_slot(document, active_document, slot);
                report.applied = inspector_report.applied;
                report.inspector_report = Some(inspector_report.clone());
                let _ = report.diagnostics.extend(inspector_report.diagnostics);
                report
            }
            EditorDocumentSlot::Prefab(slot) => {
                let patch = match command
                    .to_patch_for_document(&slot.session.document().instantiate(), registry)
                {
                    Ok(patch) => patch,
                    Err(diagnostic) => {
                        let mut report = report_for_prefab_slot(document, active_document, slot);
                        report.applied = false;
                        report.diagnostics.push(diagnostic);
                        return report;
                    }
                };
                let patch_report = slot.session.apply_patch(&patch, registry);
                let mut report = report_for_prefab_patch_slot(
                    document,
                    active_document,
                    slot,
                    patch_report.clone(),
                );
                report.inspector_report = Some(SceneInspectorCommandReport {
                    applied: patch_report.applied,
                    patch: Some(patch),
                    patch_report: Some(patch_report.clone()),
                    diagnostics: patch_report.diagnostics,
                });
                report
            }
            EditorDocumentSlot::Extension(_) => {
                authoring_command_error_report(active_document, document)
            }
        }
    }

    fn apply_authoring_inspector_command(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: SceneInspectorCommand,
    ) -> EditorWorkspaceCommandReport {
        let (active_document, slot) =
            match self.resolve_exact_authoring_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        match slot {
            EditorDocumentSlot::Scene(slot) => {
                let inspector_report = slot
                    .editor
                    .inspector()
                    .apply_authoring_command(&mut slot.session, command);
                if inspector_report.applied {
                    slot.selection.retain_existing(slot.session.document());
                }
                let mut report = report_for_slot(document, active_document, slot);
                report.applied = inspector_report.applied;
                report.inspector_report = Some(inspector_report.clone());
                let _ = report.diagnostics.extend(inspector_report.diagnostics);
                report
            }
            EditorDocumentSlot::Prefab(slot) => {
                if !matches!(
                    &command,
                    SceneInspectorCommand::SetField { .. }
                        | SceneInspectorCommand::RemoveField { .. }
                        | SceneInspectorCommand::SetAssetRefField { .. }
                ) {
                    return authoring_command_error_report(active_document, document);
                }
                let patch = match command.to_patch(&slot.session.document().instantiate()) {
                    Ok(patch) => patch,
                    Err(diagnostic) => {
                        let mut report = report_for_prefab_slot(document, active_document, slot);
                        report.applied = false;
                        report.diagnostics.push(diagnostic);
                        return report;
                    }
                };
                let patch_report = slot.session.apply_authoring_patch(&patch);
                report_for_prefab_patch_slot(document, active_document, slot, patch_report)
            }
            EditorDocumentSlot::Extension(_) => {
                authoring_command_error_report(active_document, document)
            }
        }
    }

    fn apply_scene_patch(
        &mut self,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        patch: ScenePatchDocument,
    ) -> EditorWorkspaceCommandReport {
        let (active_document, slot) =
            match self.resolve_exact_scene_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        let patch_report = slot.session.apply_patch(&patch, registry);
        report_for_patch_slot(document, active_document, slot, patch_report)
    }

    fn apply_prefab_patch(
        &mut self,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        patch: ScenePatchDocument,
    ) -> EditorWorkspaceCommandReport {
        let (active_document, slot) =
            match self.resolve_exact_prefab_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        let patch_report = slot.session.apply_patch(&patch, registry);
        report_for_prefab_patch_slot(document, active_document, slot, patch_report)
    }

    fn apply_authoring_field_edit(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        edit: AuthoringFieldEdit,
    ) -> EditorWorkspaceCommandReport {
        let (active_document, slot) =
            match self.resolve_exact_authoring_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        let patch_report = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.apply_authoring_field_edit(edit),
            EditorDocumentSlot::Prefab(slot) => slot.session.apply_authoring_field_edit(edit),
            EditorDocumentSlot::Extension(_) => {
                return authoring_command_error_report(active_document, document);
            }
        };
        report_for_document_patch(document, active_document, slot, patch_report)
    }

    fn apply_authoring_patch(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        kind: EditorDocumentKind,
        patch: ScenePatchDocument,
    ) -> EditorWorkspaceCommandReport {
        let (active_document, slot) =
            match self.resolve_exact_authoring_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        if slot.kind() != kind {
            return authoring_command_error_report(active_document, document);
        }
        let patch_report = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.apply_authoring_patch(&patch),
            EditorDocumentSlot::Prefab(slot) => slot.session.apply_authoring_patch(&patch),
            EditorDocumentSlot::Extension(_) => {
                return authoring_command_error_report(active_document, document);
            }
        };
        report_for_document_patch(document, active_document, slot, patch_report)
    }

    fn undo_authoring(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> EditorWorkspaceCommandReport {
        if matches!(
            self.document_kind(document),
            Some(EditorDocumentKind::Extension(_))
        ) {
            return self.apply_extension_history_transition(document, base_revision, false);
        }
        let (active_document, slot) =
            match self.resolve_exact_authoring_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        let patch_report = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.undo_authoring(),
            EditorDocumentSlot::Prefab(slot) => slot.session.undo_authoring(),
            EditorDocumentSlot::Extension(_) => {
                return authoring_command_error_report(active_document, document);
            }
        };
        report_for_document_patch(document, active_document, slot, patch_report)
    }

    fn redo_authoring(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> EditorWorkspaceCommandReport {
        if matches!(
            self.document_kind(document),
            Some(EditorDocumentKind::Extension(_))
        ) {
            return self.apply_extension_history_transition(document, base_revision, true);
        }
        let (active_document, slot) =
            match self.resolve_exact_authoring_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        let patch_report = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.redo_authoring(),
            EditorDocumentSlot::Prefab(slot) => slot.session.redo_authoring(),
            EditorDocumentSlot::Extension(_) => {
                return authoring_command_error_report(active_document, document);
            }
        };
        report_for_document_patch(document, active_document, slot, patch_report)
    }

    fn apply_generic_composition(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        kind: EditorDocumentKind,
        command: SceneCompositionCommand,
    ) -> EditorWorkspaceCommandReport {
        let (active_document, slot) =
            match self.resolve_exact_authoring_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        if slot.kind() != kind {
            return authoring_command_error_report(active_document, document);
        }
        if let SceneCompositionCommand::CreateEntity {
            kind: crate::SceneCompositionEntityKind::Empty,
            placement,
        } = command
        {
            let created = match slot {
                EditorDocumentSlot::Scene(slot) => slot
                    .session
                    .create_authoring_entity(base_revision, placement),
                EditorDocumentSlot::Prefab(slot) => slot
                    .session
                    .create_authoring_entity(base_revision, placement),
                EditorDocumentSlot::Extension(_) => {
                    return authoring_command_error_report(active_document, document);
                }
            };
            return match created {
                Ok(created) => {
                    let (entity, patch_report) = created.into_parts();
                    slot.selection_mut().clear();
                    let mut report =
                        report_for_document_patch(document, active_document, slot, patch_report);
                    slot.selection_mut().select_entity(Some(entity.clone()));
                    if kind == EditorDocumentKind::Prefab {
                        report.prefab_composition_outcome =
                            Some(PrefabCompositionOutcome::Created { entity });
                    }
                    report
                }
                Err(patch_report) => {
                    report_for_document_patch(document, active_document, slot, patch_report)
                }
            };
        }
        if let (
            EditorDocumentSlot::Prefab(prefab),
            SceneCompositionCommand::MoveEntity { entity, to },
        ) = (&*slot, &command)
            && to.matches_current_prefab(prefab.session.document(), entity)
        {
            let mut report = report_for_document(document, active_document, slot);
            report.applied = false;
            report.prefab_composition_outcome = Some(PrefabCompositionOutcome::NoOp {
                entity: entity.clone(),
            });
            return report;
        }
        let source = match &*slot {
            EditorDocumentSlot::Scene(slot) => {
                GenericCompositionDocument::Scene(slot.session.document())
            }
            EditorDocumentSlot::Prefab(slot) => {
                GenericCompositionDocument::Prefab(slot.session.document())
            }
            EditorDocumentSlot::Extension(_) => {
                return authoring_command_error_report(active_document, document);
            }
        };
        let prepared = prepare_generic_composition(
            source,
            slot.selection().top_entity(),
            command,
            |component| match &*slot {
                EditorDocumentSlot::Scene(slot) => {
                    slot.session.authoring_initial_component(component)
                }
                EditorDocumentSlot::Prefab(slot) => {
                    slot.session.authoring_initial_component(component)
                }
                EditorDocumentSlot::Extension(_) => None,
            },
        );
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(diagnostic) => {
                let mut report = report_for_document(document, active_document, slot);
                report.applied = false;
                report.diagnostics.push(diagnostic);
                return report;
            }
        };
        let patch_report = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.apply_authoring_patch(prepared.patch()),
            EditorDocumentSlot::Prefab(slot) => {
                slot.session.apply_authoring_patch(prepared.patch())
            }
            EditorDocumentSlot::Extension(_) => {
                return authoring_command_error_report(active_document, document);
            }
        };
        let mut report = report_for_document_patch(document, active_document, slot, patch_report);
        if report.applied {
            if let SceneCompositionSelection::Select(entity) = prepared.selection() {
                slot.selection_mut().select_entity(entity.clone());
            }
            if kind == EditorDocumentKind::Prefab {
                report.prefab_composition_outcome = match &prepared.patch().operations[0] {
                    nara_scene::ScenePatchOperation::MoveEntity { entity, .. } => {
                        Some(PrefabCompositionOutcome::Moved {
                            entity: entity.clone(),
                        })
                    }
                    nara_scene::ScenePatchOperation::RemoveEntity { entity, .. } => {
                        Some(PrefabCompositionOutcome::Removed {
                            entity: entity.clone(),
                            selection: slot.selection().top_entity().cloned(),
                        })
                    }
                    nara_scene::ScenePatchOperation::AddComponent {
                        entity, component, ..
                    } => Some(PrefabCompositionOutcome::ComponentAdded {
                        entity: entity.clone(),
                        component: component.clone(),
                    }),
                    nara_scene::ScenePatchOperation::RemoveComponent { entity, component } => {
                        Some(PrefabCompositionOutcome::ComponentRemoved {
                            entity: entity.clone(),
                            component: component.clone(),
                        })
                    }
                    _ => None,
                };
            }
        }
        report
    }

    fn apply_scene_composition(
        &mut self,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: SceneCompositionCommand,
    ) -> EditorWorkspaceCommandReport {
        let (active_document, slot) =
            match self.resolve_exact_scene_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        if let SceneCompositionCommand::CreateEntity { kind, placement } = command {
            let components = match scene_entity_components(registry, kind) {
                Ok(components) => components,
                Err(entry) => {
                    let mut report = report_for_slot(document, active_document, slot);
                    report.applied = false;
                    report.diagnostics.push(entry);
                    return report;
                }
            };
            return match slot.session.create_entity_with_registry(
                base_revision,
                registry,
                placement,
                components,
            ) {
                Ok(created) => {
                    let (entity, patch_report) = created.into_parts();
                    slot.selection.clear();
                    let report =
                        report_for_patch_slot(document, active_document, slot, patch_report);
                    slot.selection.select_entity(Some(entity));
                    report
                }
                Err(patch_report) => {
                    report_for_patch_slot(document, active_document, slot, patch_report)
                }
            };
        }
        let prepared = match prepare_scene_composition(
            slot.session.document(),
            registry,
            slot.selection.top_entity(),
            command,
        ) {
            Ok(prepared) => prepared,
            Err(entry) => {
                let mut report = report_for_slot(document, active_document, slot);
                report.applied = false;
                report.diagnostics.push(entry);
                return report;
            }
        };
        let patch_report = slot.session.apply_patch(prepared.patch(), registry);
        let report = report_for_patch_slot(document, active_document, slot, patch_report);
        if report.applied {
            match prepared.selection() {
                SceneCompositionSelection::Preserve => {}
                SceneCompositionSelection::Select(entity) => {
                    slot.selection.select_entity(entity.clone());
                }
            }
        }
        report
    }

    fn apply_prefab_composition(
        &mut self,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: PrefabCompositionCommand,
    ) -> EditorWorkspaceCommandReport {
        let (active_document, slot) =
            match self.resolve_exact_prefab_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        if let PrefabCompositionCommand::CreateEntity { draft, placement } = command {
            return match slot.session.create_entity_with_registry(
                base_revision,
                registry,
                placement,
                draft.components().clone(),
            ) {
                Ok(created) => {
                    let (entity, patch_report) = created.into_parts();
                    slot.selection.clear();
                    let mut report =
                        report_for_prefab_patch_slot(document, active_document, slot, patch_report);
                    slot.selection.select_entity(Some(entity.clone()));
                    report.prefab_composition_outcome =
                        Some(PrefabCompositionOutcome::Created { entity });
                    report
                }
                Err(patch_report) => {
                    report_for_prefab_patch_slot(document, active_document, slot, patch_report)
                }
            };
        }
        let prepared = match prepare_prefab_composition(
            slot.session.document(),
            registry,
            slot.selection.top_entity(),
            command,
        ) {
            Ok(prepared) => prepared,
            Err(entry) => {
                let mut report = report_for_prefab_slot(document, active_document, slot);
                report.applied = false;
                report.diagnostics.push(entry);
                return report;
            }
        };
        let Some(patch) = prepared.patch() else {
            let mut report = report_for_prefab_slot(document, active_document, slot);
            report.applied = false;
            report.prefab_composition_outcome = Some(prepared.outcome().clone());
            return report;
        };
        let patch_report = slot.session.apply_patch(patch, registry);
        let mut report =
            report_for_prefab_patch_slot(document, active_document, slot, patch_report);
        if report.applied {
            match prepared.selection() {
                PrefabCompositionSelection::Preserve => {}
                PrefabCompositionSelection::Select(entity) => {
                    slot.selection.select_entity(entity.clone());
                }
            }
            report.prefab_composition_outcome = Some(prepared.outcome().clone());
        }
        report
    }

    fn undo(
        &mut self,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> EditorWorkspaceCommandReport {
        if matches!(
            self.document_kind(document),
            Some(EditorDocumentKind::Extension(_))
        ) {
            return self.apply_extension_history_transition(document, base_revision, false);
        }
        let (active_document, slot) =
            match self.resolve_exact_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        let patch_report = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.undo(registry),
            EditorDocumentSlot::Prefab(slot) => slot.session.undo(registry),
            EditorDocumentSlot::Extension(_) => {
                return authoring_command_error_report(active_document, document);
            }
        };
        report_for_document_patch(document, active_document, slot, patch_report)
    }

    fn redo(
        &mut self,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> EditorWorkspaceCommandReport {
        if matches!(
            self.document_kind(document),
            Some(EditorDocumentKind::Extension(_))
        ) {
            return self.apply_extension_history_transition(document, base_revision, true);
        }
        let (active_document, slot) =
            match self.resolve_exact_document_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => return report,
            };
        let patch_report = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.redo(registry),
            EditorDocumentSlot::Prefab(slot) => slot.session.redo(registry),
            EditorDocumentSlot::Extension(_) => {
                return authoring_command_error_report(active_document, document);
            }
        };
        report_for_document_patch(document, active_document, slot, patch_report)
    }

    fn resolve_exact_document_mutation_slot(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> Result<(Option<EditorDocumentId>, &mut EditorDocumentSlot), EditorWorkspaceCommandReport>
    {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(slot) = self.documents.get_mut(&document) else {
            return Err(workspace_document_error_report(context, document));
        };
        if slot.revision() != base_revision {
            return Err(workspace_revision_error_report(
                context,
                document,
                base_revision,
                slot.revision(),
            ));
        }
        Ok((active_document, slot))
    }

    fn apply_extension_history_transition(
        &mut self,
        document: EditorDocumentId,
        base_revision: DocumentRevision,
        redo: bool,
    ) -> EditorWorkspaceCommandReport {
        let active = self.active_document;
        let context = WorkspaceDocumentContext::new(active, Some(document));
        let Some(document_slot @ EditorDocumentSlot::Extension(_)) =
            self.documents.get_mut(&document)
        else {
            return workspace_document_error_report(context, document);
        };
        let EditorDocumentSlot::Extension(slot) = document_slot else {
            unreachable!()
        };
        if slot.revision != base_revision {
            return workspace_revision_error_report(
                context,
                document,
                base_revision,
                slot.revision,
            );
        }
        let result = if redo {
            slot.owner.redo()
        } else {
            slot.owner.undo()
        };
        if let Err(error) = result {
            return extension_error_report(context, error);
        }
        slot.revision = slot.revision.next();
        report_for_document(document, active, document_slot)
    }

    fn resolve_exact_authoring_document_mutation_slot(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> Result<(Option<EditorDocumentId>, &mut EditorDocumentSlot), EditorWorkspaceCommandReport>
    {
        let active_document = self.active_document;
        let (resolved_active, slot) =
            self.resolve_exact_document_mutation_slot(document, base_revision)?;
        if slot.authoring_readiness().is_none() {
            return Err(authoring_document_error_report(active_document, document));
        }
        Ok((resolved_active, slot))
    }

    fn require_authoring_document(
        &self,
        document: EditorDocumentId,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let context = WorkspaceDocumentContext::new(self.active_document, Some(document));
        let Some(slot) = self.documents.get(&document) else {
            return Err(workspace_document_error_report(context, document));
        };
        if slot.authoring_readiness().is_none() && !matches!(slot, EditorDocumentSlot::Extension(_))
        {
            return Err(authoring_document_error_report(
                self.active_document,
                document,
            ));
        }
        Ok(())
    }

    fn resolve_exact_scene_mutation_slot(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> Result<(Option<EditorDocumentId>, &mut EditorSceneSlot), EditorWorkspaceCommandReport>
    {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let slot = match self.scene_slot_mut(document) {
            Ok(slot) => slot,
            Err(report) => return Err(report),
        };
        if slot.revision() != base_revision {
            return Err(workspace_revision_error_report(
                context,
                document,
                base_revision,
                slot.revision(),
            ));
        }
        Ok((active_document, slot))
    }

    fn resolve_exact_prefab_mutation_slot(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> Result<(Option<EditorDocumentId>, &mut EditorPrefabSlot), EditorWorkspaceCommandReport>
    {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let slot = match self.prefab_slot_mut(document) {
            Ok(slot) => slot,
            Err(report) => return Err(report),
        };
        if slot.revision() != base_revision {
            return Err(workspace_revision_error_report(
                context,
                document,
                base_revision,
                slot.revision(),
            ));
        }
        Ok((active_document, slot))
    }

    fn mark_external_changed(
        &mut self,
        document: EditorDocumentId,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(slot) = self.documents.get_mut(&document) else {
            return workspace_document_error_report(context, document);
        };
        slot.mark_external_changed();
        report_for_document(document, active_document, slot)
    }

    fn scene_slot_mut(
        &mut self,
        document: EditorDocumentId,
    ) -> Result<&mut EditorSceneSlot, EditorWorkspaceCommandReport> {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(kind) = self.documents.get(&document).map(EditorDocumentSlot::kind) else {
            return Err(workspace_document_error_report(context, document));
        };
        if kind != EditorDocumentKind::Scene {
            return Err(workspace_kind_error_report(
                context,
                document,
                EditorDocumentKind::Scene,
                kind,
            ));
        }
        match self.documents.get_mut(&document) {
            Some(EditorDocumentSlot::Scene(slot)) => Ok(slot),
            Some(EditorDocumentSlot::Prefab(_) | EditorDocumentSlot::Extension(_)) | None => {
                unreachable!("document kind changed while the workspace was mutably borrowed")
            }
        }
    }

    fn prefab_slot_mut(
        &mut self,
        document: EditorDocumentId,
    ) -> Result<&mut EditorPrefabSlot, EditorWorkspaceCommandReport> {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(kind) = self.documents.get(&document).map(EditorDocumentSlot::kind) else {
            return Err(workspace_document_error_report(context, document));
        };
        if kind != EditorDocumentKind::Prefab {
            return Err(workspace_kind_error_report(
                context,
                document,
                EditorDocumentKind::Prefab,
                kind,
            ));
        }
        match self.documents.get_mut(&document) {
            Some(EditorDocumentSlot::Prefab(slot)) => Ok(slot),
            Some(EditorDocumentSlot::Scene(_) | EditorDocumentSlot::Extension(_)) | None => {
                unreachable!("document kind changed while the workspace was mutably borrowed")
            }
        }
    }

    pub fn reload_external_session(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        session: SceneAuthoringSession,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        let (active_document, slot) =
            match self.resolve_exact_scene_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => {
                    return Err(EditorSceneSessionPublicationError::new(report, session));
                }
            };
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        if slot.session.live_instance().is_some() {
            return Err(EditorSceneSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-current-session-attached",
                    "external scene reload requires the current authoring session to detach first",
                ),
                session,
            ));
        }
        if session.live_instance().is_some() {
            return Err(EditorSceneSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-session-attached",
                    "workspace publication requires a detached scene authoring session",
                ),
                session,
            ));
        };
        if slot.is_dirty() {
            slot.external_reload = EditorExternalReloadState::Conflict;
            return Err(EditorSceneSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-reload-conflict",
                    "external scene reload cannot replace a dirty editor document",
                ),
                session,
            ));
        }
        slot.session = session;
        slot.selection.clear();
        slot.editor = SceneEditorState::new();
        slot.saved_revision = slot.session.revision();
        slot.saved_content_state = slot.session.content_state();
        slot.saved_digest = None;
        slot.external_reload = EditorExternalReloadState::Clean;
        Ok(report_for_slot(document, active_document, slot))
    }

    /// Publishes bytes explicitly reopened by the concrete persistence Host.
    ///
    /// Unlike an unsolicited external reload, an explicit Reopen/Reconcile command is allowed to
    /// replace a dirty authoring session. The Host has already read and validated the candidate;
    /// this method only performs the workspace publication and resets authoring-local state.
    fn publish_reopened_session(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        session: SceneAuthoringSession,
        digest: EditorDocumentDigest,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        let (active_document, slot) =
            match self.resolve_exact_scene_mutation_slot(document, base_revision) {
                Ok(exact) => exact,
                Err(report) => {
                    return Err(EditorSceneSessionPublicationError::new(report, session));
                }
            };
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        if slot.session.live_instance().is_some() {
            return Err(EditorSceneSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-current-session-attached",
                    "reopen requires the current authoring session to detach first",
                ),
                session,
            ));
        }
        if session.live_instance().is_some() {
            return Err(EditorSceneSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-session-attached",
                    "workspace publication requires a detached reopened session",
                ),
                session,
            ));
        }

        slot.session = session;
        slot.selection.clear();
        slot.editor = SceneEditorState::new();
        slot.saved_revision = slot.session.revision();
        slot.saved_content_state = slot.session.content_state();
        slot.saved_digest = Some(digest);
        slot.external_reload = EditorExternalReloadState::Clean;
        Ok(report_for_slot(document, active_document, slot))
    }

    fn publish_reopened_prefab_session(
        &mut self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        source_binding: EditorDocumentSourceBinding,
        session: PrefabAuthoringSession,
        digest: EditorDocumentDigest,
    ) -> Result<EditorWorkspaceCommandReport, EditorPrefabSessionPublicationError> {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(EditorDocumentSlot::Prefab(slot)) = self.documents.get_mut(&document) else {
            let report = if let Some(actual) =
                self.documents.get(&document).map(EditorDocumentSlot::kind)
            {
                workspace_kind_error_report(context, document, EditorDocumentKind::Prefab, actual)
            } else {
                workspace_document_error_report(context, document)
            };
            return Err(EditorPrefabSessionPublicationError::new(report, session));
        };
        if slot.revision() != base_revision || slot.source_binding() != &source_binding {
            return Err(EditorPrefabSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-prefab-reopen-mismatch",
                    "reopened Prefab source or base revision does not match the open slot",
                ),
                session,
            ));
        }

        slot.session = session;
        slot.selection.clear();
        slot.saved_revision = slot.session.revision();
        slot.saved_content_state = slot.session.content_state();
        slot.saved_digest = Some(digest);
        slot.external_reload = EditorExternalReloadState::Clean;
        Ok(EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document),
            active_document,
            dirty: Some(slot.is_dirty()),
            revision: Some(slot.revision()),
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        })
    }

    fn allocate_document_id(&mut self) -> EditorDocumentId {
        self.next_document_id = self.next_document_id.saturating_add(1);
        EditorDocumentId::from_raw(self.next_document_id)
    }
}

#[derive(Debug)]
pub struct EditorSceneSlot {
    title: String,
    source_binding: Option<EditorDocumentSourceBinding>,
    session: SceneAuthoringSession,
    editor: SceneEditorState,
    selection: EditorSelectionSet,
    saved_revision: SceneAuthoringRevision,
    saved_content_state: SceneAuthoringContentState,
    saved_digest: Option<EditorDocumentDigest>,
    external_reload: EditorExternalReloadState,
}

impl EditorSceneSlot {
    fn from_session(
        title: impl Into<String>,
        source_binding: Option<EditorDocumentSourceBinding>,
        session: SceneAuthoringSession,
    ) -> Self {
        let saved_revision = session.revision();
        let saved_content_state = session.content_state();
        Self {
            title: title.into(),
            source_binding,
            session,
            editor: SceneEditorState::new(),
            selection: EditorSelectionSet::default(),
            saved_revision,
            saved_content_state,
            saved_digest: None,
            external_reload: EditorExternalReloadState::Clean,
        }
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn source_binding(&self) -> Option<&EditorDocumentSourceBinding> {
        self.source_binding.as_ref()
    }

    #[must_use]
    pub fn session(&self) -> &SceneAuthoringSession {
        &self.session
    }

    #[must_use]
    pub fn editor(&self) -> &SceneEditorState {
        &self.editor
    }

    #[must_use]
    pub fn selection(&self) -> &EditorSelectionSet {
        &self.selection
    }

    #[must_use]
    pub fn revision(&self) -> SceneAuthoringRevision {
        self.session.revision()
    }

    #[must_use]
    pub fn saved_revision(&self) -> SceneAuthoringRevision {
        self.saved_revision
    }

    #[must_use]
    pub fn saved_content_state(&self) -> SceneAuthoringContentState {
        self.saved_content_state
    }

    #[must_use]
    pub const fn saved_digest(&self) -> Option<EditorDocumentDigest> {
        self.saved_digest
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.session.content_state() != self.saved_content_state
            || self.session.source_upgrade_required()
    }

    #[must_use]
    pub fn external_reload(&self) -> EditorExternalReloadState {
        self.external_reload
    }

    #[must_use]
    pub fn authoring_readiness(&self) -> Option<AuthoringDocumentReadiness> {
        self.session.authoring_readiness()
    }

    #[must_use]
    pub fn authoring_record_facts(&self) -> Option<&[AuthoringRecordFact]> {
        self.session.authoring_record_facts()
    }

    #[must_use]
    pub fn unavailable_record_manifest(&self) -> Option<&UnavailableRecordManifest> {
        self.session.unavailable_record_manifest()
    }

    fn apply_persistence_checkpoint(
        &mut self,
        revision: SceneAuthoringRevision,
        content_state: SceneAuthoringContentState,
        digest: EditorDocumentDigest,
    ) {
        self.saved_revision = revision;
        self.saved_content_state = content_state;
        self.saved_digest = Some(digest);
        let acknowledged = self.session.acknowledge_source_saved(content_state);
        debug_assert!(
            acknowledged,
            "checkpoint source was validated before commit"
        );
        if self.external_reload != EditorExternalReloadState::Conflict {
            self.external_reload = EditorExternalReloadState::Clean;
        }
    }

    fn model(
        &self,
        document: EditorDocumentId,
        registry: &ComponentRegistry,
        edit_world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> EditorSceneModel {
        let editor = self.editor.model_with_selection(
            &self.session,
            registry,
            self.selection.top_entity(),
            edit_world_snapshot,
        );
        EditorSceneModel {
            document,
            kind: EditorDocumentKind::Scene,
            title: self.title.clone(),
            dirty: self.is_dirty(),
            revision: self.session.revision(),
            saved_revision: self.saved_revision,
            external_reload: self.external_reload,
            selection: self.selection.clone(),
            editor,
        }
    }

    fn authoring_model(&self, document: EditorDocumentId) -> EditorSceneModel {
        let editor = SceneEditorModel {
            inspector: self
                .editor
                .inspector()
                .authoring_model_with_selection(&self.session, self.selection.top_entity()),
            diagnostics: DiagnosticReport::default(),
        };
        EditorSceneModel {
            document,
            kind: EditorDocumentKind::Scene,
            title: self.title.clone(),
            dirty: self.is_dirty(),
            revision: self.session.revision(),
            saved_revision: self.saved_revision,
            external_reload: self.external_reload,
            selection: self.selection.clone(),
            editor,
        }
    }
}

#[derive(Debug)]
pub struct EditorPrefabSlot {
    title: String,
    source_binding: EditorDocumentSourceBinding,
    session: PrefabAuthoringSession,
    selection: EditorSelectionSet,
    saved_revision: SceneAuthoringRevision,
    saved_content_state: SceneAuthoringContentState,
    saved_digest: Option<EditorDocumentDigest>,
    external_reload: EditorExternalReloadState,
}

impl EditorPrefabSlot {
    fn model(
        &self,
        document: EditorDocumentId,
        registry: Option<&ComponentRegistry>,
    ) -> EditorSceneModel {
        let inspector = crate::SceneInspectorState::new().prefab_model_with_selection(
            &self.session,
            registry,
            self.selection.top_entity(),
        );
        EditorSceneModel {
            document,
            kind: EditorDocumentKind::Prefab,
            title: self.title.clone(),
            dirty: self.is_dirty(),
            revision: self.revision(),
            saved_revision: self.saved_revision(),
            external_reload: self.external_reload(),
            selection: self.selection.clone(),
            editor: SceneEditorModel {
                inspector,
                diagnostics: DiagnosticReport::default(),
            },
        }
    }
    fn from_session(
        title: impl Into<String>,
        source_binding: EditorDocumentSourceBinding,
        session: PrefabAuthoringSession,
    ) -> Self {
        let saved_revision = session.revision();
        let saved_content_state = session.content_state();
        Self {
            title: title.into(),
            source_binding,
            session,
            selection: EditorSelectionSet::default(),
            saved_revision,
            saved_content_state,
            saved_digest: None,
            external_reload: EditorExternalReloadState::Clean,
        }
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn source_binding(&self) -> &EditorDocumentSourceBinding {
        &self.source_binding
    }

    #[must_use]
    pub fn session(&self) -> &PrefabAuthoringSession {
        &self.session
    }

    #[must_use]
    pub fn selection(&self) -> &EditorSelectionSet {
        &self.selection
    }

    #[must_use]
    pub fn revision(&self) -> SceneAuthoringRevision {
        self.session.revision()
    }

    #[must_use]
    pub fn saved_revision(&self) -> SceneAuthoringRevision {
        self.saved_revision
    }

    #[must_use]
    pub fn saved_content_state(&self) -> SceneAuthoringContentState {
        self.saved_content_state
    }

    #[must_use]
    pub const fn saved_digest(&self) -> Option<EditorDocumentDigest> {
        self.saved_digest
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.session.content_state() != self.saved_content_state
            || self.session.source_upgrade_required()
    }

    #[must_use]
    pub fn external_reload(&self) -> EditorExternalReloadState {
        self.external_reload
    }

    #[must_use]
    pub fn authoring_readiness(&self) -> Option<AuthoringDocumentReadiness> {
        self.session.authoring_readiness()
    }

    #[must_use]
    pub fn authoring_record_facts(&self) -> Option<&[AuthoringRecordFact]> {
        self.session.authoring_record_facts()
    }

    #[must_use]
    pub fn unavailable_record_manifest(&self) -> Option<&UnavailableRecordManifest> {
        self.session.unavailable_record_manifest()
    }

    fn apply_persistence_checkpoint(
        &mut self,
        revision: SceneAuthoringRevision,
        content_state: SceneAuthoringContentState,
        digest: EditorDocumentDigest,
    ) {
        self.saved_revision = revision;
        self.saved_content_state = content_state;
        self.saved_digest = Some(digest);
        let acknowledged = self.session.acknowledge_source_saved(content_state);
        debug_assert!(
            acknowledged,
            "checkpoint source was validated before commit"
        );
        if self.external_reload != EditorExternalReloadState::Conflict {
            self.external_reload = EditorExternalReloadState::Clean;
        }
    }
}

impl EditorWorkspace {
    fn publish_reopened_extension_document(
        &mut self,
        document: EditorDocumentId,
        base_revision: DocumentRevision,
        source_binding: &EditorDocumentSourceBinding,
        candidate: EditorExtensionDocumentCandidate,
        digest: EditorDocumentDigest,
    ) -> Result<EditorWorkspaceCommandReport, EditorWorkspaceCommandReport> {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(EditorDocumentSlot::Extension(slot)) = self.documents.get_mut(&document) else {
            return Err(workspace_document_error_report(context, document));
        };
        if slot.owner.rust_type_id() != candidate.0.rust_type_id()
            || slot.revision != base_revision
            || &slot.source_binding != source_binding
        {
            return Err(workspace_error_report(
                context,
                "tooling.extension-document-reopen-stale",
                "reopened document no longer matches the open source and revision",
            ));
        }
        slot.owner = candidate.0;
        slot.revision = DocumentRevision::initial();
        slot.saved_revision = slot.revision;
        slot.saved_content_state = slot.owner.content_state();
        slot.saved_digest = Some(digest);
        slot.external_reload = EditorExternalReloadState::Clean;
        Ok(EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document),
            active_document,
            dirty: Some(false),
            revision: Some(slot.revision),
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        })
    }
}

impl EditorPersistenceAuthority {
    /// Captures the current revision of one document for a concrete persistence Host.
    #[must_use]
    pub fn capture(
        &mut self,
        workspace: &EditorWorkspace,
        document: EditorDocumentId,
    ) -> Option<EditorPersistenceCheckpoint> {
        if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            return None;
        }
        let slot = workspace.documents.get(&document)?;
        let (kind, source_binding, revision, content_state, content) = match slot {
            EditorDocumentSlot::Scene(slot) => (
                EditorDocumentKind::Scene,
                slot.source_binding.clone(),
                slot.revision(),
                slot.session.content_state(),
                EditorPersistenceContent::Scene(slot.session.document().clone()),
            ),
            EditorDocumentSlot::Prefab(slot) => (
                EditorDocumentKind::Prefab,
                Some(slot.source_binding.clone()),
                slot.revision(),
                slot.session.content_state(),
                EditorPersistenceContent::Prefab(slot.session.document().clone()),
            ),
            EditorDocumentSlot::Extension(slot) => (
                EditorDocumentKind::Extension(slot.owner.type_id()),
                Some(slot.source_binding.clone()),
                slot.revision,
                slot.owner.content_state(),
                EditorPersistenceContent::Extension(slot.owner.encode()),
            ),
        };
        Some(EditorPersistenceCheckpoint {
            identity: Arc::clone(&self.identity),
            document,
            kind,
            source_binding,
            revision,
            content_state,
            content,
        })
    }

    /// Commits a captured checkpoint backed by accepted filesystem publication evidence.
    ///
    /// The checkpoint may lag the current revision when an edit was accepted while a save was in
    /// flight. It may never target another workspace or authoring source, jump ahead, or regress an
    /// already accepted checkpoint.
    pub fn commit(
        &mut self,
        workspace: &mut EditorWorkspace,
        commit: EditorPersistenceCommit,
    ) -> EditorWorkspaceCommandReport {
        let EditorPersistenceCommit { checkpoint, digest } = commit;
        if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity)
            || !Arc::ptr_eq(&checkpoint.identity, &workspace.persistence_identity)
        {
            let context =
                WorkspaceDocumentContext::new(workspace.active_document, Some(checkpoint.document));
            return workspace_error_report(
                context,
                "tooling.workspace-persistence-authority-mismatch",
                "persistence checkpoint does not belong to the hosted workspace authority",
            );
        }
        workspace.apply_persistence_checkpoint(
            checkpoint.document,
            checkpoint.kind,
            checkpoint.source_binding.as_ref(),
            checkpoint.revision,
            checkpoint.content_state,
            digest,
        )
    }

    /// Binds bytes read by the concrete Host when it first opened a document.
    #[doc(hidden)]
    pub fn bind_opened_source_digest(
        &mut self,
        workspace: &mut EditorWorkspace,
        document: EditorDocumentId,
        digest: EditorDocumentDigest,
    ) -> EditorWorkspaceCommandReport {
        if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            let context = WorkspaceDocumentContext::new(workspace.active_document, Some(document));
            return workspace_error_report(
                context,
                "tooling.workspace-persistence-authority-mismatch",
                "opened source digest does not belong to the hosted workspace authority",
            );
        }
        workspace.bind_opened_source_digest(document, digest)
    }

    /// Publishes a document explicitly reopened and validated by the concrete Host.
    #[doc(hidden)]
    pub fn publish_reopened_session(
        &mut self,
        workspace: &mut EditorWorkspace,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        session: SceneAuthoringSession,
        digest: EditorDocumentDigest,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            let context = WorkspaceDocumentContext::new(workspace.active_document, Some(document));
            return Err(EditorSceneSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-persistence-authority-mismatch",
                    "reopened session does not belong to the hosted workspace authority",
                ),
                session,
            ));
        }
        workspace.publish_reopened_session(document, base_revision, session, digest)
    }

    /// Publishes a Prefab explicitly reopened and validated by the concrete Host.
    #[doc(hidden)]
    pub fn publish_reopened_prefab_session(
        &mut self,
        workspace: &mut EditorWorkspace,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        source_binding: EditorDocumentSourceBinding,
        session: PrefabAuthoringSession,
        digest: EditorDocumentDigest,
    ) -> Result<EditorWorkspaceCommandReport, EditorPrefabSessionPublicationError> {
        if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            let context = WorkspaceDocumentContext::new(workspace.active_document, Some(document));
            return Err(EditorPrefabSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-persistence-authority-mismatch",
                    "reopened Prefab session does not belong to the hosted workspace authority",
                ),
                session,
            ));
        }
        workspace.publish_reopened_prefab_session(
            document,
            base_revision,
            source_binding,
            session,
            digest,
        )
    }

    /// Publishes a typed extension codec result only against its captured open revision.
    #[doc(hidden)]
    pub fn publish_reopened_extension_document(
        &mut self,
        workspace: &mut EditorWorkspace,
        document: EditorDocumentId,
        base_revision: DocumentRevision,
        source_binding: &EditorDocumentSourceBinding,
        candidate: EditorExtensionDocumentCandidate,
        digest: EditorDocumentDigest,
    ) -> Result<EditorWorkspaceCommandReport, EditorWorkspaceCommandReport> {
        if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            let context = WorkspaceDocumentContext::new(workspace.active_document, Some(document));
            return Err(workspace_error_report(
                context,
                "tooling.workspace-persistence-authority-mismatch",
                "reopened extension document does not belong to the hosted workspace authority",
            ));
        }
        workspace.publish_reopened_extension_document(
            document,
            base_revision,
            source_binding,
            candidate,
            digest,
        )
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EditorSelectionSet {
    entities: BTreeSet<SceneEntityId>,
    top_entity: Option<SceneEntityId>,
}

impl EditorSelectionSet {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    #[must_use]
    pub fn top_entity(&self) -> Option<&SceneEntityId> {
        self.top_entity.as_ref()
    }

    pub fn entities(&self) -> impl Iterator<Item = &SceneEntityId> {
        self.entities.iter()
    }

    pub fn clear(&mut self) {
        self.entities.clear();
        self.top_entity = None;
    }

    pub fn select_entity(&mut self, entity: Option<SceneEntityId>) {
        self.clear();
        if let Some(entity) = entity {
            self.entities.insert(entity.clone());
            self.top_entity = Some(entity);
        }
    }

    pub fn retain_existing(&mut self, document: &SceneDocument) {
        self.entities
            .retain(|entity| document_has_entity(document, entity));
        if self
            .top_entity
            .as_ref()
            .is_some_and(|entity| !document_has_entity(document, entity))
        {
            self.top_entity = self.entities.iter().next().cloned();
        }
    }

    fn retain_existing_prefab(&mut self, document: &PrefabDocument) {
        self.entities
            .retain(|entity| prefab_has_entity(document, entity));
        if self
            .top_entity
            .as_ref()
            .is_some_and(|entity| !prefab_has_entity(document, entity))
        {
            self.top_entity = self.entities.iter().next().cloned();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExternalReloadState {
    Clean,
    Pending,
    Conflict,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorWorkspaceCommand {
    CloseDocument {
        document: EditorDocumentId,
    },
    SetActiveDocument {
        document: EditorDocumentId,
    },
    SelectEntity {
        document: EditorDocumentId,
        entity: Option<SceneEntityId>,
    },
    ApplyInspectorCommand {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: SceneInspectorCommand,
    },
    ApplyScenePatch {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        patch: ScenePatchDocument,
    },
    ApplyPrefabPatch {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        patch: ScenePatchDocument,
    },
    ApplyAuthoringFieldEdit {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        edit: AuthoringFieldEdit,
    },
    ComposeScene {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: SceneCompositionCommand,
    },
    ComposePrefab {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: PrefabCompositionCommand,
    },
    Undo {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    },
    Redo {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    },
    MarkExternalChanged {
        document: EditorDocumentId,
    },
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct EditorWorkspaceCommandReport {
    pub applied: bool,
    pub document: Option<EditorDocumentId>,
    pub opened_document: Option<EditorDocumentId>,
    pub active_document: Option<EditorDocumentId>,
    pub dirty: Option<bool>,
    pub revision: Option<SceneAuthoringRevision>,
    pub inspector_report: Option<SceneInspectorCommandReport>,
    pub patch_report: Option<ScenePatchReport>,
    pub prefab_composition_outcome: Option<PrefabCompositionOutcome>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug)]
pub struct EditorSceneSessionPublicationError {
    report: Box<EditorWorkspaceCommandReport>,
    session: Box<SceneAuthoringSession>,
}

impl EditorSceneSessionPublicationError {
    fn new(report: EditorWorkspaceCommandReport, session: SceneAuthoringSession) -> Self {
        Self {
            report: Box::new(report),
            session: Box::new(session),
        }
    }

    #[must_use]
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    #[must_use]
    pub fn into_session(self) -> SceneAuthoringSession {
        *self.session
    }

    #[must_use]
    pub fn into_parts(self) -> (EditorWorkspaceCommandReport, SceneAuthoringSession) {
        (*self.report, *self.session)
    }
}

impl Display for EditorSceneSessionPublicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("scene authoring session was not published to the editor workspace")
    }
}

impl Error for EditorSceneSessionPublicationError {}

#[derive(Debug)]
pub struct EditorPrefabSessionPublicationError {
    report: Box<EditorWorkspaceCommandReport>,
    session: Box<PrefabAuthoringSession>,
}

impl EditorPrefabSessionPublicationError {
    fn new(report: EditorWorkspaceCommandReport, session: PrefabAuthoringSession) -> Self {
        Self {
            report: Box::new(report),
            session: Box::new(session),
        }
    }

    #[must_use]
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    #[must_use]
    pub fn into_session(self) -> PrefabAuthoringSession {
        *self.session
    }

    #[must_use]
    pub fn into_parts(self) -> (EditorWorkspaceCommandReport, PrefabAuthoringSession) {
        (*self.report, *self.session)
    }
}

impl Display for EditorPrefabSessionPublicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("prefab authoring session was not published to the editor workspace")
    }
}

impl Error for EditorPrefabSessionPublicationError {}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorWorkspaceModel {
    pub active_document: Option<EditorDocumentId>,
    pub documents: Vec<EditorDocumentTabModel>,
    pub scenes: Vec<EditorSceneTabModel>,
    pub active_scene: Option<EditorSceneModel>,
    pub extension_documents: Vec<EditorExtensionDocumentView>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorDocumentTabModel {
    pub document: EditorDocumentId,
    pub kind: EditorDocumentKind,
    pub title: String,
    pub dirty: bool,
    pub revision: SceneAuthoringRevision,
    pub saved_revision: SceneAuthoringRevision,
    pub history: SceneAuthoringHistoryStatus,
    pub authoring_readiness: Option<AuthoringDocumentReadiness>,
    /// Immutable schema and Adapter admission; document/source incarnation is `revision.source_id()`.
    pub authoring_schema_basis: Option<AuthoringSchemaBasis>,
    pub source_binding: Option<EditorDocumentSourceBinding>,
    pub external_reload: EditorExternalReloadState,
    pub active: bool,
}

impl EditorWorkspaceModel {
    /// Returns the exact facts for an open document in this model generation.
    #[must_use]
    pub fn document(&self, document: EditorDocumentId) -> Option<&EditorDocumentTabModel> {
        self.documents
            .iter()
            .find(|candidate| candidate.document == document)
    }

    /// Returns the exact Scene tab facts for an open document in this model generation.
    #[must_use]
    pub fn scene(&self, document: EditorDocumentId) -> Option<&EditorSceneTabModel> {
        self.scenes.iter().find(|scene| scene.document == document)
    }

    #[must_use]
    pub fn extension_document(
        &self,
        document: EditorDocumentId,
    ) -> Option<&EditorExtensionDocumentView> {
        self.extension_documents
            .iter()
            .find(|view| view.document() == document)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSceneTabModel {
    pub document: EditorDocumentId,
    pub title: String,
    pub dirty: bool,
    pub revision: SceneAuthoringRevision,
    pub external_reload: EditorExternalReloadState,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorSceneModel {
    pub document: EditorDocumentId,
    pub kind: EditorDocumentKind,
    pub title: String,
    pub dirty: bool,
    pub revision: SceneAuthoringRevision,
    pub saved_revision: SceneAuthoringRevision,
    pub external_reload: EditorExternalReloadState,
    pub selection: EditorSelectionSet,
    pub editor: SceneEditorModel,
}

fn report_for_slot(
    document: EditorDocumentId,
    active_document: Option<EditorDocumentId>,
    slot: &EditorSceneSlot,
) -> EditorWorkspaceCommandReport {
    EditorWorkspaceCommandReport {
        applied: true,
        document: Some(document),
        active_document,
        dirty: Some(slot.is_dirty()),
        revision: Some(slot.revision()),
        diagnostics: DiagnosticReport::default(),
        ..EditorWorkspaceCommandReport::default()
    }
}

fn report_for_prefab_slot(
    document: EditorDocumentId,
    active_document: Option<EditorDocumentId>,
    slot: &EditorPrefabSlot,
) -> EditorWorkspaceCommandReport {
    EditorWorkspaceCommandReport {
        applied: true,
        document: Some(document),
        active_document,
        dirty: Some(slot.is_dirty()),
        revision: Some(slot.revision()),
        diagnostics: DiagnosticReport::default(),
        ..EditorWorkspaceCommandReport::default()
    }
}

fn report_for_document(
    document: EditorDocumentId,
    active_document: Option<EditorDocumentId>,
    slot: &EditorDocumentSlot,
) -> EditorWorkspaceCommandReport {
    EditorWorkspaceCommandReport {
        applied: true,
        document: Some(document),
        active_document,
        dirty: Some(slot.is_dirty()),
        revision: Some(slot.revision()),
        diagnostics: DiagnosticReport::default(),
        ..EditorWorkspaceCommandReport::default()
    }
}

fn report_for_patch_slot(
    document: EditorDocumentId,
    active_document: Option<EditorDocumentId>,
    slot: &mut EditorSceneSlot,
    patch_report: ScenePatchReport,
) -> EditorWorkspaceCommandReport {
    if patch_report.applied {
        slot.selection.retain_existing(slot.session.document());
    }
    let applied = patch_report.applied;
    let diagnostics = patch_report.diagnostics.clone();
    let mut report = report_for_slot(document, active_document, slot);
    report.applied = applied;
    report.patch_report = Some(patch_report);
    let _ = report.diagnostics.extend(diagnostics);
    report
}

fn report_for_prefab_patch_slot(
    document: EditorDocumentId,
    active_document: Option<EditorDocumentId>,
    slot: &mut EditorPrefabSlot,
    patch_report: ScenePatchReport,
) -> EditorWorkspaceCommandReport {
    if patch_report.applied {
        slot.selection
            .retain_existing_prefab(slot.session.document());
    }
    let applied = patch_report.applied;
    let diagnostics = patch_report.diagnostics.clone();
    let mut report = EditorWorkspaceCommandReport {
        applied,
        document: Some(document),
        active_document,
        dirty: Some(slot.is_dirty()),
        revision: Some(slot.revision()),
        patch_report: Some(patch_report),
        diagnostics: DiagnosticReport::default(),
        ..EditorWorkspaceCommandReport::default()
    };
    let _ = report.diagnostics.extend(diagnostics);
    report
}

fn report_for_document_patch(
    document: EditorDocumentId,
    active_document: Option<EditorDocumentId>,
    slot: &mut EditorDocumentSlot,
    patch_report: ScenePatchReport,
) -> EditorWorkspaceCommandReport {
    if patch_report.applied {
        slot.retain_existing_selection();
    }
    let applied = patch_report.applied;
    let diagnostics = patch_report.diagnostics.clone();
    let mut report = report_for_document(document, active_document, slot);
    report.applied = applied;
    report.patch_report = Some(patch_report);
    let _ = report.diagnostics.extend(diagnostics);
    report
}

fn workspace_error_report(
    context: WorkspaceDocumentContext,
    code: &'static str,
    summary: &'static str,
) -> EditorWorkspaceCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(with_workspace_document_context(
        diagnostic::error(code, summary),
        context,
    ));
    EditorWorkspaceCommandReport {
        applied: false,
        document: context.target,
        active_document: context.active,
        diagnostics,
        ..EditorWorkspaceCommandReport::default()
    }
}

fn extension_error_report(
    context: WorkspaceDocumentContext,
    error: EditorExtensionDocumentError,
) -> EditorWorkspaceCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(with_workspace_document_context(error.diagnostic(), context));
    EditorWorkspaceCommandReport {
        applied: false,
        document: context.target,
        active_document: context.active,
        diagnostics,
        ..EditorWorkspaceCommandReport::default()
    }
}

fn authoring_document_error_report(
    active_document: Option<EditorDocumentId>,
    document: EditorDocumentId,
) -> EditorWorkspaceCommandReport {
    workspace_error_report(
        WorkspaceDocumentContext::new(active_document, Some(document)),
        "tooling.workspace-authoring-document-required",
        "generic authoring commands require a published authoring document",
    )
}

fn authoring_command_error_report(
    active_document: Option<EditorDocumentId>,
    document: EditorDocumentId,
) -> EditorWorkspaceCommandReport {
    workspace_error_report(
        WorkspaceDocumentContext::new(active_document, Some(document)),
        "tooling.workspace-authoring-command-unsupported",
        "generic authoring does not accept this document kind or composition command",
    )
}

fn workspace_document_error_report(
    context: WorkspaceDocumentContext,
    document: EditorDocumentId,
) -> EditorWorkspaceCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::with_public_u64(
        with_workspace_document_context(
            diagnostic::error(
                "tooling.workspace-missing-document",
                "workspace command targets a document that is not open",
            ),
            context,
        ),
        "missing_document",
        document.raw(),
    ));
    EditorWorkspaceCommandReport {
        applied: false,
        document: Some(document),
        active_document: context.active,
        diagnostics,
        ..EditorWorkspaceCommandReport::default()
    }
}

fn workspace_kind_error_report(
    context: WorkspaceDocumentContext,
    document: EditorDocumentId,
    expected: EditorDocumentKind,
    actual: EditorDocumentKind,
) -> EditorWorkspaceCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    let diagnostic = diagnostic::with_public_identifier(
        diagnostic::with_public_identifier(
            with_workspace_document_context(
                diagnostic::error(
                    "tooling.workspace-document-kind-mismatch",
                    "workspace command targets a document of the wrong kind",
                ),
                context,
            ),
            "expected_kind",
            document_kind_name(expected),
        ),
        "actual_kind",
        document_kind_name(actual),
    );
    diagnostics.push(diagnostic);
    EditorWorkspaceCommandReport {
        applied: false,
        document: Some(document),
        active_document: context.active,
        diagnostics,
        ..EditorWorkspaceCommandReport::default()
    }
}

fn document_kind_name(kind: EditorDocumentKind) -> &'static str {
    match kind {
        EditorDocumentKind::Scene => "scene",
        EditorDocumentKind::Prefab => "prefab",
        EditorDocumentKind::Extension(type_id) => type_id,
    }
}

fn workspace_revision_error_report(
    context: WorkspaceDocumentContext,
    document: EditorDocumentId,
    base_revision: SceneAuthoringRevision,
    current_revision: SceneAuthoringRevision,
) -> EditorWorkspaceCommandReport {
    let diagnostic = diagnostic::with_public_u64(
        diagnostic::with_public_u64(
            with_workspace_document_context(
                diagnostic::error(
                    "tooling.workspace-stale-document-revision",
                    "workspace mutation targets a stale document revision",
                ),
                context,
            ),
            "base_generation",
            base_revision.generation(),
        ),
        "current_generation",
        current_revision.generation(),
    );
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic);
    EditorWorkspaceCommandReport {
        applied: false,
        document: Some(document),
        active_document: context.active,
        revision: Some(current_revision),
        diagnostics,
        ..EditorWorkspaceCommandReport::default()
    }
}

fn workspace_entity_error_report(
    context: WorkspaceDocumentContext,
    document: EditorDocumentId,
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
) -> EditorWorkspaceCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::with_entity(
        with_workspace_document_context(diagnostic::error(code, summary), context),
        entity,
    ));
    EditorWorkspaceCommandReport {
        applied: false,
        document: Some(document),
        active_document: context.active,
        diagnostics,
        ..EditorWorkspaceCommandReport::default()
    }
}

fn with_workspace_document_context(
    mut entry: nara_diagnostic::Diagnostic,
    context: WorkspaceDocumentContext,
) -> nara_diagnostic::Diagnostic {
    if let Some(active) = context.active {
        entry = diagnostic::with_public_u64(entry, "active_document", active.raw());
    }
    if let Some(requested) = context.requested {
        entry = diagnostic::with_public_u64(entry, "requested_document", requested.raw());
    }
    if let Some(target) = context.target {
        entry = diagnostic::with_public_u64(entry, "target_document", target.raw());
    }
    entry
}

fn document_has_entity(document: &SceneDocument, entity: &SceneEntityId) -> bool {
    document.entities.iter().any(|record| record.id == *entity)
}

fn prefab_has_entity(document: &PrefabDocument, entity: &SceneEntityId) -> bool {
    document.entities.iter().any(|record| record.id == *entity)
}

fn prefab_field_value<'a>(
    workspace: &'a EditorWorkspace,
    registry: &'a ComponentRegistry,
    target: &EditorFieldTarget,
    field: &nara_reflect::ComponentFieldId,
) -> Option<&'a nara_reflect::ComponentValue> {
    let slot = workspace.prefab(target.document())?;
    if slot.revision() != target.base_revision() {
        return None;
    }
    let entity = slot
        .session()
        .document()
        .entities
        .iter()
        .find(|entity| &entity.id == target.entity())?;
    let component = entity.components.get(target.component())?;
    if component.version != target.component_version() {
        return None;
    }
    let schema = registry.schema(target.component())?;
    if schema.version() != target.component_version() {
        return None;
    }
    let field = schema
        .fields()
        .iter()
        .find(|candidate| candidate.id() == field)?;
    component.value.get_path(&field.path).ok()
}

#[cfg(test)]
mod tests {
    mod generic_structure;
    mod prefab_declarations;

    use super::*;
    use nara_asset::AssetRef;
    use nara_diagnostic::DiagnosticValueRef;
    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentRegistry,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, PersistentComponentSchemaOwnerId,
    };
    use nara_scene::{
        AuthoringDocumentSource, PrefabDocument, PrefabDocumentCandidate,
        SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentRecord,
        SceneDocumentCandidate, SceneEntityRecord, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    };

    use crate::{
        PrefabCompositionCommand, PrefabCompositionEntityDraft, PrefabCompositionOutcome,
        SceneCompositionEntityKind,
    };

    mod entity_queries {
        use super::*;
        use crate::{
            EditorDocumentQueryBasis, EditorEntityQueryLimits, MAX_EDITOR_ENTITY_QUERY_ROWS,
            MAX_EDITOR_ENTITY_QUERY_SCAN_RECORDS,
        };

        fn start(
            workspace: &EditorWorkspace,
            document: EditorDocumentId,
            scan: usize,
            rows: usize,
        ) -> EditorEntityQuery {
            let slot = workspace.documents.get(&document).unwrap();
            EditorEntityQuery {
                position: EditorEntityQueryPosition::Start(EditorDocumentQueryBasis {
                    document,
                    revision: slot.revision(),
                    schema: slot.authoring_schema_basis(),
                }),
                limits: EditorEntityQueryLimits::new(scan, rows).unwrap(),
            }
        }

        fn resume(cursor: EditorEntityCursor, scan: usize, rows: usize) -> EditorEntityQuery {
            EditorEntityQuery {
                position: EditorEntityQueryPosition::Continue(cursor),
                limits: EditorEntityQueryLimits::new(scan, rows).unwrap(),
            }
        }

        fn records() -> Vec<SceneEntityRecord> {
            ["a", "b", "c", "d", "e"]
                .map(|id| SceneEntityRecord::new(entity_id(id)))
                .into()
        }

        fn facts(
            workspace: &EditorWorkspace,
            document: EditorDocumentId,
        ) -> (
            SceneAuthoringRevision,
            DocumentContentState,
            DocumentHistoryStatus,
            EditorSelectionSet,
            bool,
        ) {
            let slot = workspace.documents.get(&document).unwrap();
            let (content, selection) = match slot {
                EditorDocumentSlot::Scene(slot) => {
                    (slot.session().content_state(), slot.selection().clone())
                }
                EditorDocumentSlot::Prefab(slot) => {
                    (slot.session().content_state(), slot.selection().clone())
                }
                EditorDocumentSlot::Extension(_) => panic!("expected an entity document"),
            };
            (
                slot.revision(),
                content,
                slot.history_status(),
                selection,
                slot.is_dirty(),
            )
        }

        #[test]
        fn scan_and_row_limits_page_scene_and_prefab_source_records_and_replay_exactly() {
            for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
                let mut workspace = EditorWorkspace::new();
                let document = match kind {
                    EditorDocumentKind::Scene => open_scene(
                        &mut workspace,
                        "scene",
                        SceneDocument::from_ordered_roots(records()),
                    ),
                    EditorDocumentKind::Prefab => open_prefab(
                        &mut workspace,
                        "prefab",
                        PrefabDocument::from_ordered_roots(records()),
                    ),
                    EditorDocumentKind::Extension(_) => unreachable!(),
                };
                let first = workspace
                    .query_document_entities(start(&workspace, document, 2, 4))
                    .unwrap();
                assert_eq!(first.entities, [entity_id("a"), entity_id("b")]);
                assert_eq!(first.scanned_records, 2);
                let request = resume(first.next.unwrap(), 4, 1);
                let middle = workspace.query_document_entities(request.clone()).unwrap();
                assert_eq!(middle.entities, [entity_id("c")]);
                assert_eq!(middle.scanned_records, 1);
                assert_eq!(workspace.query_document_entities(request).unwrap(), middle);
                let last = workspace
                    .query_document_entities(resume(middle.next.unwrap(), 4, 4))
                    .unwrap();
                assert_eq!(last.entities, [entity_id("d"), entity_id("e")]);
                assert_eq!(last.scanned_records, 2);
                assert!(last.next.is_none());
            }
        }

        #[test]
        fn zero_and_over_cap_limits_reject_while_exact_caps_are_valid() {
            for (scan, rows) in [
                (0, 1),
                (1, 0),
                (MAX_EDITOR_ENTITY_QUERY_SCAN_RECORDS + 1, 1),
                (1, MAX_EDITOR_ENTITY_QUERY_ROWS + 1),
                (usize::MAX, usize::MAX),
            ] {
                assert_eq!(
                    EditorEntityQueryLimits::new(scan, rows),
                    Err(EditorDocumentQueryRejection::InvalidLimits {
                        max_scan_records: scan,
                        max_rows: rows,
                    })
                );
            }
            assert!(
                EditorEntityQueryLimits::new(
                    MAX_EDITOR_ENTITY_QUERY_SCAN_RECORDS,
                    MAX_EDITOR_ENTITY_QUERY_ROWS,
                )
                .is_ok()
            );
        }

        #[test]
        fn empty_sources_complete_and_unknown_documents_reject_without_falling_back_to_active() {
            let mut workspace = EditorWorkspace::new();
            for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
                let document = match kind {
                    EditorDocumentKind::Scene => {
                        open_scene(&mut workspace, "empty", SceneDocument::default())
                    }
                    EditorDocumentKind::Prefab => {
                        open_prefab(&mut workspace, "empty", PrefabDocument::default())
                    }
                    EditorDocumentKind::Extension(_) => unreachable!(),
                };
                let before = facts(&workspace, document);
                let request = start(&workspace, document, 1, 1);
                let page = workspace.query_document_entities(request.clone()).unwrap();
                assert!(page.entities.is_empty());
                assert_eq!(page.scanned_records, 0);
                assert!(page.next.is_none());
                assert_eq!(facts(&workspace, document), before);
                let unknown = EditorDocumentId::from_raw(u64::MAX);
                let EditorEntityQueryPosition::Start(mut basis) = request.position else {
                    unreachable!();
                };
                basis.document = unknown;
                assert_eq!(
                    workspace.query_document_entities(EditorEntityQuery {
                        position: EditorEntityQueryPosition::Start(basis),
                        limits: request.limits,
                    }),
                    Err(EditorDocumentQueryRejection::DocumentNotOpen { document: unknown })
                );
                assert_eq!(workspace.active_document(), Some(document));
                assert_eq!(facts(&workspace, document), before);
            }
        }

        #[test]
        fn querying_non_active_scene_and_prefab_preserves_both_selections_and_document_state() {
            let registry = frozen_empty_registry();
            for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
                let mut workspace = EditorWorkspace::new();
                let a = open_scene(
                    &mut workspace,
                    "a",
                    scene_with_entity(entity_id("selected-a")),
                );
                let b = match kind {
                    EditorDocumentKind::Scene => open_scene(
                        &mut workspace,
                        "b",
                        SceneDocument::from_ordered_roots(records()),
                    ),
                    EditorDocumentKind::Prefab => open_prefab(
                        &mut workspace,
                        "b",
                        PrefabDocument::from_ordered_roots(records()),
                    ),
                    EditorDocumentKind::Extension(_) => unreachable!(),
                };
                for (document, entity) in [(a, "selected-a"), (b, "c")] {
                    assert!(
                        workspace
                            .apply_command(
                                &registry,
                                EditorWorkspaceCommand::SelectEntity {
                                    document,
                                    entity: Some(entity_id(entity)),
                                },
                            )
                            .applied
                    );
                }
                assert!(workspace.activate_document(a).applied);
                let before = [facts(&workspace, a), facts(&workspace, b)];
                let mut page = workspace
                    .query_document_entities(start(&workspace, b, 2, 2))
                    .unwrap();
                while let Some(cursor) = page.next {
                    page = workspace
                        .query_document_entities(resume(cursor, 2, 2))
                        .unwrap();
                }
                assert_eq!(workspace.active_document(), Some(a));
                assert_eq!([facts(&workspace, a), facts(&workspace, b)], before);
            }
        }

        #[test]
        fn foreign_owner_rejects_before_document_lookup_even_for_matching_raw_document_ids() {
            let mut original = EditorWorkspace::new();
            let document = open_scene(
                &mut original,
                "original",
                SceneDocument::from_ordered_roots(records()),
            );
            let cursor = original
                .query_document_entities(start(&original, document, 1, 1))
                .unwrap()
                .next
                .unwrap();
            let mut other = EditorWorkspace::new();
            assert_eq!(
                other.query_document_entities(resume(cursor.clone(), 1, 1)),
                Err(EditorDocumentQueryRejection::WrongWorkspace)
            );
            let other_document = open_scene(&mut other, "other", SceneDocument::default());
            assert_eq!(document, other_document);
            assert_eq!(
                other.query_document_entities(resume(cursor, 1, 1)),
                Err(EditorDocumentQueryRejection::WrongWorkspace)
            );
        }

        #[test]
        fn changes_to_a_do_not_invalidate_b_but_b_edit_and_undo_do() {
            let registry = frozen_empty_registry();
            let mut workspace = EditorWorkspace::new();
            let a = open_scene(&mut workspace, "a", SceneDocument::default());
            let b = open_scene(
                &mut workspace,
                "b",
                SceneDocument::from_ordered_roots(records()),
            );
            let original_b = workspace.scene(b).unwrap().session().document().clone();
            let cursor = workspace
                .query_document_entities(start(&workspace, b, 1, 1))
                .unwrap()
                .next
                .unwrap();
            assert!(apply_patch(&mut workspace, &registry, a, add_entity_patch("new-a")).applied);
            assert!(workspace.activate_document(a).applied);
            let before_a = facts(&workspace, a);
            let before_b = facts(&workspace, b);
            assert!(
                workspace
                    .query_document_entities(resume(cursor.clone(), 1, 1))
                    .is_ok()
            );
            assert_eq!(facts(&workspace, a), before_a);
            assert_eq!(facts(&workspace, b), before_b);
            assert!(apply_patch(&mut workspace, &registry, b, add_entity_patch("new-b")).applied);
            let edited_b = facts(&workspace, b);
            assert_eq!(
                workspace.query_document_entities(resume(cursor.clone(), 1, 1)),
                Err(EditorDocumentQueryRejection::StaleRevision)
            );
            assert_eq!(facts(&workspace, b), edited_b);
            assert!(undo(&mut workspace, &registry, b).applied);
            assert_eq!(
                workspace.scene(b).unwrap().session().document(),
                &original_b
            );
            assert_eq!(
                workspace.query_document_entities(resume(cursor, 1, 1)),
                Err(EditorDocumentQueryRejection::StaleRevision)
            );
            assert_eq!(workspace.active_document(), Some(a));
        }

        #[test]
        fn closed_document_and_same_slot_replacement_do_not_silently_rebase_continuations() {
            let registry = frozen_empty_registry();
            let mut workspace = EditorWorkspace::new();
            let document = open_scene(
                &mut workspace,
                "b",
                SceneDocument::from_ordered_roots(records()),
            );
            let request = start(&workspace, document, 1, 1);
            let cursor = workspace
                .query_document_entities(request.clone())
                .unwrap()
                .next
                .unwrap();
            let revision = workspace.document_revision(document).unwrap();
            workspace
                .reload_external_session(
                    document,
                    revision,
                    SceneAuthoringSession::new(SceneDocument::from_ordered_roots(records())),
                )
                .unwrap();
            let before = facts(&workspace, document);
            assert_eq!(
                workspace.query_document_entities(resume(cursor.clone(), 1, 1)),
                Err(EditorDocumentQueryRejection::StaleIncarnation)
            );
            assert_eq!(
                workspace.query_document_entities(request),
                Err(EditorDocumentQueryRejection::StaleIncarnation)
            );
            assert_eq!(facts(&workspace, document), before);
            assert!(
                workspace
                    .apply_command(
                        &registry,
                        EditorWorkspaceCommand::CloseDocument { document },
                    )
                    .applied
            );
            assert_eq!(
                workspace.query_document_entities(resume(cursor, 1, 1)),
                Err(EditorDocumentQueryRejection::DocumentNotOpen { document })
            );
        }

        #[test]
        fn schema_admission_is_exact_and_source_revision_checks_precede_it() {
            let bundle = authoring_basis_bundle("same catalog");
            let mut workspace = EditorWorkspace::new();
            let first = workspace
                .open_authoring_scene_document(
                    "first",
                    EditorDocumentSourceBinding::new("scenes/first.scene.json"),
                    published_basis_scene(&bundle),
                )
                .unwrap()
                .opened_document
                .unwrap();
            let second = workspace
                .open_authoring_scene_document(
                    "second",
                    EditorDocumentSourceBinding::new("scenes/second.scene.json"),
                    published_basis_scene(&bundle),
                )
                .unwrap()
                .opened_document
                .unwrap();
            assert!(
                workspace
                    .query_document_entities(start(&workspace, first, 1, 1))
                    .is_ok()
            );
            let mut request = start(&workspace, first, 1, 1);
            let EditorEntityQueryPosition::Start(basis) = &mut request.position else {
                unreachable!();
            };
            basis.schema = workspace
                .scene(second)
                .unwrap()
                .session()
                .authoring_schema_basis();
            assert_eq!(
                workspace.query_document_entities(request.clone()),
                Err(EditorDocumentQueryRejection::StaleSchema)
            );
            let EditorEntityQueryPosition::Start(basis) = &mut request.position else {
                unreachable!();
            };
            basis.schema = None;
            assert_eq!(
                workspace.query_document_entities(request.clone()),
                Err(EditorDocumentQueryRejection::StaleSchema)
            );
            let EditorEntityQueryPosition::Start(basis) = &mut request.position else {
                unreachable!();
            };
            basis.revision = basis.revision.next();
            assert_eq!(
                workspace.query_document_entities(request.clone()),
                Err(EditorDocumentQueryRejection::StaleRevision)
            );
            let EditorEntityQueryPosition::Start(basis) = &mut request.position else {
                unreachable!();
            };
            basis.revision = workspace.document_revision(second).unwrap();
            assert_eq!(
                workspace.query_document_entities(request),
                Err(EditorDocumentQueryRejection::StaleIncarnation)
            );
        }
    }

    #[test]
    fn generated_entity_identity_does_not_restart_when_source_history_reopens() {
        let registry = composition_registry();
        let mut workspace = EditorWorkspace::new();
        let document = open_scene(
            &mut workspace,
            "fresh-source-identity",
            SceneDocument::default(),
        );
        let create = || SceneCompositionCommand::CreateEntity {
            kind: SceneCompositionEntityKind::Empty,
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        };
        assert!(compose(&mut workspace, &registry, document, create()).applied);
        let first = workspace
            .scene(document)
            .unwrap()
            .selection()
            .top_entity()
            .unwrap()
            .clone();
        assert!(undo(&mut workspace, &registry, document).applied);
        assert!(
            workspace
                .scene(document)
                .unwrap()
                .session()
                .document()
                .entities
                .is_empty()
        );
        assert!(!workspace.scene(document).unwrap().is_dirty());
        let revision = workspace.document_revision(document).unwrap();
        workspace
            .reload_external_session(
                document,
                revision,
                SceneAuthoringSession::new(SceneDocument::default()),
            )
            .unwrap();
        assert!(compose(&mut workspace, &registry, document, create()).applied);
        let second = workspace
            .scene(document)
            .unwrap()
            .selection()
            .top_entity()
            .unwrap();
        assert_ne!(
            &first, second,
            "ordinary creation must not systematically assert the old object's identity after Reopen"
        );
    }

    #[test]
    fn composition_commands_form_one_ordered_atomic_authoring_trajectory() {
        let registry = composition_registry();
        let mut workspace = EditorWorkspace::new();
        let document = open_scene(&mut workspace, "composition", SceneDocument::default());

        assert!(
            compose(
                &mut workspace,
                &registry,
                document,
                SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::Empty,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            )
            .applied
        );
        let root = workspace
            .scene(document)
            .unwrap()
            .selection()
            .top_entity()
            .unwrap()
            .clone();
        assert_eq!(
            workspace
                .scene(document)
                .unwrap()
                .session()
                .document()
                .roots,
            [root.clone()]
        );

        assert!(
            compose(
                &mut workspace,
                &registry,
                document,
                SceneCompositionCommand::AddComponent {
                    entity: root.clone(),
                    component: ComponentTypeId::new("nara.transform.Transform2d"),
                },
            )
            .applied
        );
        assert!(
            compose(
                &mut workspace,
                &registry,
                document,
                SceneCompositionCommand::RemoveComponent {
                    entity: root.clone(),
                    component: ComponentTypeId::new("nara.transform.Transform2d"),
                },
            )
            .applied
        );

        let history_before_sprite = workspace
            .scene(document)
            .unwrap()
            .session()
            .history_status()
            .undo_depth;
        assert!(
            compose(
                &mut workspace,
                &registry,
                document,
                SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::ColoredSprite,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            )
            .applied
        );
        let slot = workspace.scene(document).unwrap();
        let sprite = slot.selection().top_entity().unwrap().clone();
        assert_ne!(sprite, root);
        assert_eq!(
            slot.session().history_status().undo_depth,
            history_before_sprite + 1
        );
        let sprite_record = slot
            .session()
            .document()
            .entities
            .iter()
            .find(|record| record.id == sprite)
            .unwrap();
        assert!(
            sprite_record
                .components
                .contains_key(&ComponentTypeId::new("nara.transform.Transform2d"))
        );
        assert!(
            sprite_record
                .components
                .contains_key(&ComponentTypeId::new("nara.sprite.Sprite"))
        );

        assert!(
            compose(
                &mut workspace,
                &registry,
                document,
                SceneCompositionCommand::MoveEntity {
                    entity: sprite.clone(),
                    to: ScenePlacement::last(ScenePlacementOwner::Parent {
                        parent: root.clone(),
                    }),
                },
            )
            .applied
        );
        let rows = &workspace
            .model(&registry, None)
            .active_scene
            .unwrap()
            .editor
            .inspector
            .entities;
        assert_eq!(
            rows.iter()
                .map(|row| (&row.id, row.parent.as_ref(), row.depth))
                .collect::<Vec<_>>(),
            vec![(&root, None, 0), (&sprite, Some(&root), 1)]
        );

        assert!(
            compose(
                &mut workspace,
                &registry,
                document,
                SceneCompositionCommand::RemoveEntity {
                    entity: sprite.clone(),
                },
            )
            .applied
        );
        assert_eq!(
            workspace.scene(document).unwrap().selection().top_entity(),
            Some(&root)
        );
        assert!(undo(&mut workspace, &registry, document).applied);
        assert!(document_has_entity(
            workspace.scene(document).unwrap().session().document(),
            &sprite
        ));
    }

    #[test]
    fn composition_rejection_preserves_revision_history_and_selection() {
        let empty_registry = frozen_empty_registry();
        let mut workspace = EditorWorkspace::new();
        let document = open_scene(&mut workspace, "composition", SceneDocument::default());
        let initial_revision = workspace.scene(document).unwrap().revision();

        let unavailable = compose(
            &mut workspace,
            &empty_registry,
            document,
            SceneCompositionCommand::CreateEntity {
                kind: SceneCompositionEntityKind::ColoredSprite,
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        );
        assert!(!unavailable.applied);
        assert_eq!(
            unavailable
                .diagnostics
                .iter()
                .next()
                .unwrap()
                .code()
                .as_str(),
            "tooling.composition-component-unavailable"
        );
        let slot = workspace.scene(document).unwrap();
        assert_eq!(slot.revision(), initial_revision);
        assert_eq!(slot.session().history_status(), Default::default());
        assert!(slot.selection().is_empty());

        let registry = composition_registry();
        assert!(
            compose(
                &mut workspace,
                &registry,
                document,
                SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::Empty,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            )
            .applied
        );
        let allocated = workspace
            .scene(document)
            .unwrap()
            .selection()
            .top_entity()
            .unwrap()
            .clone();
        let current_revision = workspace.scene(document).unwrap().revision();
        let current_history = workspace
            .scene(document)
            .unwrap()
            .session()
            .history_status();
        let stale = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ComposeScene {
                document,
                base_revision: initial_revision,
                command: SceneCompositionCommand::AddComponent {
                    entity: allocated.clone(),
                    component: ComponentTypeId::new("nara.transform.Transform2d"),
                },
            },
        );
        assert!(!stale.applied);
        assert_eq!(
            stale.diagnostics.iter().next().unwrap().code().as_str(),
            "tooling.workspace-stale-document-revision"
        );
        let slot = workspace.scene(document).unwrap();
        assert_eq!(slot.revision(), current_revision);
        assert_eq!(slot.session().history_status(), current_history);
        assert_eq!(slot.selection().top_entity(), Some(&allocated));
    }

    #[test]
    fn prefab_composition_returns_exact_identity_and_rejects_without_publication() {
        let registry = frozen_empty_registry();
        let existing = entity_id("editor_prefab_entity_1");
        let mut workspace = EditorWorkspace::new();
        let document = open_prefab(
            &mut workspace,
            "composition",
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(existing.clone())]),
        );
        let initial_revision = workspace.prefab(document).unwrap().revision();
        let draft = PrefabCompositionEntityDraft::new(BTreeMap::new()).unwrap();

        let created = compose_prefab(
            &mut workspace,
            &registry,
            document,
            PrefabCompositionCommand::CreateEntity {
                draft: draft.clone(),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        );
        assert!(created.applied);
        let Some(PrefabCompositionOutcome::Created { entity: first }) =
            created.prefab_composition_outcome
        else {
            panic!("successful creation must return its exact identity");
        };
        assert_ne!(first, existing);
        assert_eq!(
            workspace
                .prefab(document)
                .unwrap()
                .session()
                .document()
                .roots,
            [existing, first.clone()]
        );
        assert_eq!(
            workspace.prefab(document).unwrap().selection().top_entity(),
            Some(&first)
        );

        let after_first = workspace.prefab(document).unwrap().revision();
        let history_after_first = workspace
            .prefab(document)
            .unwrap()
            .session()
            .history_status();
        let rejected = compose_prefab(
            &mut workspace,
            &registry,
            document,
            PrefabCompositionCommand::CreateEntity {
                draft: draft.clone(),
                placement: ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: entity_id("missing-parent"),
                }),
            },
        );
        assert!(!rejected.applied);
        let stale = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ComposePrefab {
                document,
                base_revision: initial_revision,
                command: PrefabCompositionCommand::CreateEntity {
                    draft: draft.clone(),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            },
        );
        assert!(!stale.applied);
        let slot = workspace.prefab(document).unwrap();
        assert_eq!(slot.revision(), after_first);
        assert_eq!(slot.session().history_status(), history_after_first);
        assert_eq!(slot.selection().top_entity(), Some(&first));

        let second = compose_prefab(
            &mut workspace,
            &registry,
            document,
            PrefabCompositionCommand::CreateEntity {
                draft,
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        );
        assert!(second.applied);
        let Some(PrefabCompositionOutcome::Created { entity: second }) =
            second.prefab_composition_outcome
        else {
            panic!("successful creation must return its exact identity");
        };
        assert_ne!(first, second);
        assert_eq!(
            workspace.prefab(document).unwrap().selection().top_entity(),
            Some(&second)
        );
        assert_eq!(
            workspace
                .prefab(document)
                .unwrap()
                .session()
                .document()
                .entities
                .len(),
            3
        );
    }

    #[test]
    fn prefab_composition_move_delete_no_op_and_history_preserve_selection_contract() {
        let registry = frozen_empty_registry();
        let root = entity_id("root");
        let first = entity_id("first");
        let child = entity_id("first/child");
        let second = entity_id("second");
        let original = PrefabDocument::new(
            [root.clone()],
            [
                SceneEntityRecord::new(root.clone()).with_children([first.clone(), second.clone()]),
                SceneEntityRecord::new(first.clone()).with_children([child.clone()]),
                SceneEntityRecord::new(child.clone()),
                SceneEntityRecord::new(second.clone()),
            ],
        );
        let mut workspace = EditorWorkspace::new();
        let document = open_prefab(&mut workspace, "composition", original.clone());
        assert!(
            workspace
                .apply_command(
                    &registry,
                    EditorWorkspaceCommand::SelectEntity {
                        document,
                        entity: Some(second.clone()),
                    },
                )
                .applied
        );

        let moved = compose_prefab(
            &mut workspace,
            &registry,
            document,
            PrefabCompositionCommand::MoveEntity {
                entity: second.clone(),
                to: ScenePlacement::first(ScenePlacementOwner::Parent {
                    parent: root.clone(),
                }),
            },
        );
        assert!(moved.applied);
        assert_eq!(
            workspace.prefab(document).unwrap().selection().top_entity(),
            Some(&second)
        );
        let move_revision = workspace.prefab(document).unwrap().revision();
        assert!(
            workspace
                .apply_command(
                    &registry,
                    EditorWorkspaceCommand::Undo {
                        document,
                        base_revision: move_revision,
                    },
                )
                .applied
        );
        assert_eq!(
            workspace.prefab(document).unwrap().session().document(),
            &original
        );

        let revision_before_no_op = workspace.prefab(document).unwrap().revision();
        let history_before_no_op = workspace
            .prefab(document)
            .unwrap()
            .session()
            .history_status();
        let no_op = compose_prefab(
            &mut workspace,
            &registry,
            document,
            PrefabCompositionCommand::MoveEntity {
                entity: second.clone(),
                to: ScenePlacement::after(
                    ScenePlacementOwner::Parent {
                        parent: root.clone(),
                    },
                    first.clone(),
                ),
            },
        );
        assert!(!no_op.applied);
        assert_eq!(
            no_op.prefab_composition_outcome,
            Some(PrefabCompositionOutcome::NoOp {
                entity: second.clone()
            })
        );
        assert_eq!(
            workspace.prefab(document).unwrap().revision(),
            revision_before_no_op
        );
        assert_eq!(
            workspace
                .prefab(document)
                .unwrap()
                .session()
                .history_status(),
            history_before_no_op
        );

        assert!(
            workspace
                .apply_command(
                    &registry,
                    EditorWorkspaceCommand::SelectEntity {
                        document,
                        entity: Some(child),
                    },
                )
                .applied
        );
        let removed = compose_prefab(
            &mut workspace,
            &registry,
            document,
            PrefabCompositionCommand::RemoveEntity {
                entity: first.clone(),
            },
        );
        assert!(removed.applied);
        assert_eq!(
            workspace.prefab(document).unwrap().selection().top_entity(),
            Some(&root)
        );
        let delete_revision = workspace.prefab(document).unwrap().revision();
        assert!(
            workspace
                .apply_command(
                    &registry,
                    EditorWorkspaceCommand::Undo {
                        document,
                        base_revision: delete_revision,
                    },
                )
                .applied
        );
        assert_eq!(
            workspace.prefab(document).unwrap().session().document(),
            &original
        );
        assert_eq!(
            workspace.prefab(document).unwrap().selection().top_entity(),
            Some(&root)
        );
    }

    #[test]
    fn dirty_close_requires_an_explicit_decision() {
        let registry = frozen_empty_registry();
        let mut workspace = EditorWorkspace::new();
        let opened = workspace
            .open_scene_session("main", SceneAuthoringSession::new(SceneDocument::default()))
            .unwrap();
        let document = opened.opened_document.unwrap();

        let edited = apply_patch(
            &mut workspace,
            &registry,
            document,
            add_entity_patch("dirty"),
        );
        assert!(edited.applied);

        let close = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::CloseDocument { document },
        );
        assert!(!close.applied);
        assert_eq!(workspace.len(), 1);
        assert_eq!(workspace.active_document(), Some(document));
        assert_eq!(
            close.diagnostics.iter().next().unwrap().code().as_str(),
            "tooling.workspace-dirty-close-decision-required"
        );

        let discarded = workspace.__discard_document(document);
        assert!(discarded.applied);
        assert!(workspace.is_empty());
    }

    #[test]
    fn persistence_checkpoint_advances_only_the_captured_revision() {
        let registry = frozen_empty_registry();
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let opened = workspace
            .open_scene_session("main", SceneAuthoringSession::new(SceneDocument::default()))
            .unwrap();
        let document = opened.opened_document.unwrap();
        let stale = authority.capture(&workspace, document).unwrap();

        assert!(
            apply_patch(
                &mut workspace,
                &registry,
                document,
                add_entity_patch("captured")
            )
            .applied
        );
        let captured_revision = workspace.scene(document).unwrap().revision();
        let accepted_checkpoint = authority.capture(&workspace, document).unwrap();
        assert!(
            apply_patch(
                &mut workspace,
                &registry,
                document,
                add_entity_patch("later")
            )
            .applied
        );

        let digest = EditorDocumentDigest::new(7, [0x5a; 32]);
        let accepted = authority.commit(
            &mut workspace,
            EditorPersistenceCommit {
                checkpoint: accepted_checkpoint,
                digest,
            },
        );
        assert!(accepted.applied);
        let slot = workspace.scene(document).unwrap();
        assert_eq!(slot.saved_revision(), captured_revision);
        assert_eq!(slot.saved_digest(), Some(digest));
        assert!(slot.is_dirty());

        let stale_report = authority.commit(
            &mut workspace,
            EditorPersistenceCommit {
                checkpoint: stale,
                digest: EditorDocumentDigest::new(3, [0x11; 32]),
            },
        );
        assert!(!stale_report.applied);
        assert_eq!(
            stale_report
                .diagnostics
                .iter()
                .next()
                .unwrap()
                .code()
                .as_str(),
            "tooling.workspace-persistence-checkpoint-mismatch"
        );
        assert_eq!(
            workspace.scene(document).unwrap().saved_revision(),
            captured_revision
        );
    }

    #[test]
    fn authoring_basis_survives_edit_and_save_while_reopen_changes_source_incarnation() {
        let bundle = authoring_basis_bundle("Basis");
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = workspace
            .open_authoring_scene_document(
                "basis",
                EditorDocumentSourceBinding::new("scenes/basis.scene.json"),
                published_basis_scene(&bundle),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let initial = workspace
            .authoring_model()
            .document(document)
            .unwrap()
            .clone();
        assert!(initial.authoring_schema_basis.is_some());
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::ApplyScenePatch {
                    document,
                    base_revision: initial.revision,
                    patch: add_entity_patch("added"),
                })
                .applied
        );
        let edited = workspace
            .authoring_model()
            .document(document)
            .unwrap()
            .clone();
        assert_ne!(edited.revision, initial.revision);
        assert_eq!(edited.revision.source_id(), initial.revision.source_id());
        assert_eq!(
            edited.authoring_schema_basis,
            initial.authoring_schema_basis
        );

        let checkpoint = authority.capture(&workspace, document).unwrap();
        let digest = EditorDocumentDigest::new(7, [0x5a; 32]);
        assert!(
            authority
                .commit(
                    &mut workspace,
                    EditorPersistenceCommit { checkpoint, digest }
                )
                .applied
        );
        let saved = workspace
            .authoring_model()
            .document(document)
            .unwrap()
            .clone();
        assert_eq!(saved.revision, edited.revision);
        assert_eq!(saved.authoring_schema_basis, initial.authoring_schema_basis);
        assert!(!saved.dirty);

        let session = workspace.scene(document).unwrap().session();
        let candidate = SceneDocumentCandidate::decode_json_bytes(
            session.document().to_json_string().unwrap().as_bytes(),
        )
        .unwrap();
        let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
        authority
            .publish_reopened_session(&mut workspace, document, saved.revision, reopened, digest)
            .unwrap();
        let current = workspace
            .authoring_model()
            .document(document)
            .unwrap()
            .clone();
        assert_eq!(current.source_binding, initial.source_binding);
        assert_eq!(
            current.authoring_schema_basis,
            initial.authoring_schema_basis
        );
        assert_ne!(current.revision.source_id(), initial.revision.source_id());
    }

    #[test]
    fn equal_source_paths_do_not_alias_independent_schema_admissions() {
        let original = authoring_basis_bundle("Basis");
        let changed = authoring_basis_bundle("Renamed Basis");
        assert_ne!(original.fingerprint(), changed.fingerprint());
        let mut workspace = EditorWorkspace::new();
        let documents = [&original, &original, &changed].map(|bundle| {
            workspace
                .open_authoring_scene_document(
                    "basis",
                    EditorDocumentSourceBinding::new("scenes/basis.scene.json"),
                    published_basis_scene(bundle),
                )
                .unwrap()
                .opened_document
                .unwrap()
        });
        let model = workspace.authoring_model();
        let first = model.document(documents[0]).unwrap();
        for document in &documents[1..] {
            let other = model.document(*document).unwrap();
            assert_eq!(other.source_binding, first.source_binding);
            assert_ne!(other.authoring_schema_basis, first.authoring_schema_basis);
            assert_ne!(other.revision.source_id(), first.revision.source_id());
        }
        let native = open_scene(&mut workspace, "native", SceneDocument::default());
        assert!(
            workspace
                .authoring_model()
                .document(native)
                .unwrap()
                .authoring_schema_basis
                .is_none()
        );
    }

    #[test]
    fn prefab_projection_preserves_schema_basis_when_reopened_source_changes() {
        let bundle = authoring_basis_bundle("Basis");
        let source = EditorDocumentSourceBinding::new("prefabs/basis.prefab.json");
        let published = PrefabDocumentCandidate::decode_json_bytes(
            PrefabDocument::default()
                .to_json_string()
                .unwrap()
                .as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path(source.locator()).unwrap()),
            &bundle,
            &[],
        )
        .unwrap()
        .publish();
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = workspace
            .open_authoring_prefab_document("basis", source.clone(), published)
            .unwrap()
            .opened_document
            .unwrap();
        let initial = workspace
            .authoring_model()
            .document(document)
            .unwrap()
            .clone();
        assert!(initial.authoring_schema_basis.is_some());
        let session = workspace.prefab(document).unwrap().session();
        let candidate = PrefabDocumentCandidate::decode_json_bytes(
            session.document().to_json_string().unwrap().as_bytes(),
        )
        .unwrap();
        let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
        authority
            .publish_reopened_prefab_session(
                &mut workspace,
                document,
                initial.revision,
                source,
                reopened,
                EditorDocumentDigest::new(7, [0x5a; 32]),
            )
            .unwrap();
        let current = workspace
            .authoring_model()
            .document(document)
            .unwrap()
            .clone();
        assert_eq!(
            current.authoring_schema_basis,
            initial.authoring_schema_basis
        );
        assert_ne!(current.revision.source_id(), initial.revision.source_id());
    }

    #[test]
    fn persistence_authority_and_checkpoint_must_belong_to_the_workspace() {
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = workspace
            .open_scene_session("main", SceneAuthoringSession::new(SceneDocument::default()))
            .unwrap()
            .opened_document
            .unwrap();
        let local_checkpoint = authority.capture(&workspace, document).unwrap();
        let saved_revision = workspace.scene(document).unwrap().saved_revision();
        let (mut foreign_workspace, mut foreign_authority) = EditorWorkspace::new_hosted();
        let foreign_document = foreign_workspace
            .open_scene_session(
                "foreign",
                SceneAuthoringSession::new(SceneDocument::default()),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let foreign_checkpoint = foreign_authority
            .capture(&foreign_workspace, foreign_document)
            .unwrap();

        let report = foreign_authority.commit(
            &mut workspace,
            EditorPersistenceCommit {
                checkpoint: local_checkpoint,
                digest: EditorDocumentDigest::new(7, [0x5a; 32]),
            },
        );

        assert!(!report.applied);
        assert_eq!(
            report.diagnostics.iter().next().unwrap().code().as_str(),
            "tooling.workspace-persistence-authority-mismatch"
        );
        assert_eq!(
            workspace.scene(document).unwrap().saved_revision(),
            saved_revision
        );

        let report = authority.commit(
            &mut workspace,
            EditorPersistenceCommit {
                checkpoint: foreign_checkpoint,
                digest: EditorDocumentDigest::new(8, [0x6b; 32]),
            },
        );
        assert!(!report.applied);
        assert_eq!(
            report.diagnostics.iter().next().unwrap().code().as_str(),
            "tooling.workspace-persistence-authority-mismatch"
        );
        assert_eq!(
            workspace.scene(document).unwrap().saved_revision(),
            saved_revision
        );
    }

    #[test]
    fn two_open_scene_slots_keep_selection_and_dirty_isolated() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let hero = entity_id("hero");
        let enemy = entity_id("enemy");
        let first = open_scene(&mut workspace, "first", scene_with_entity(hero.clone()));
        let second = open_scene(&mut workspace, "second", scene_with_entity(enemy.clone()));

        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document: first,
                entity: Some(hero.clone()),
            },
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document: second,
                entity: Some(enemy.clone()),
            },
        );
        apply_patch(&mut workspace, &registry, first, add_entity_patch("prop"));

        let first_slot = workspace.scene(first).unwrap();
        let second_slot = workspace.scene(second).unwrap();
        assert_eq!(first_slot.selection().top_entity(), Some(&hero));
        assert_eq!(second_slot.selection().top_entity(), Some(&enemy));
        assert!(first_slot.is_dirty());
        assert!(!second_slot.is_dirty());
    }

    #[test]
    fn scene_and_prefab_slots_keep_selection_and_identity_isolated() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let shared_id = entity_id("retry");
        let scene = open_scene(
            &mut workspace,
            "startup",
            scene_with_entity(shared_id.clone()),
        );
        let prefab = workspace
            .open_prefab_session(
                "ui",
                EditorDocumentSourceBinding::new("prefabs/reference-ui.prefab.json"),
                PrefabAuthoringSession::new(PrefabDocument::from_ordered_roots([
                    SceneEntityRecord::new(shared_id.clone()),
                ])),
            )
            .unwrap()
            .opened_document
            .unwrap();

        assert!(workspace.scene(scene).is_some());
        assert!(workspace.prefab(prefab).is_some());
        assert_eq!(workspace.len(), 2);
        assert_eq!(workspace.model(&registry, None).documents.len(), 2);

        let scene_selection = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document: scene,
                entity: Some(shared_id.clone()),
            },
        );
        let prefab_selection = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document: prefab,
                entity: Some(shared_id.clone()),
            },
        );
        assert!(scene_selection.applied);
        assert!(prefab_selection.applied);
        assert_eq!(
            workspace.scene(scene).unwrap().selection().top_entity(),
            Some(&shared_id)
        );
        assert_eq!(
            workspace.prefab(prefab).unwrap().selection().top_entity(),
            Some(&shared_id)
        );

        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let prefab = workspace
            .open_prefab_session(
                "ui",
                EditorDocumentSourceBinding::new("prefabs/reference-ui.prefab.json"),
                PrefabAuthoringSession::new(PrefabDocument::default()),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let checkpoint = authority.capture(&workspace, prefab).unwrap();
        assert_eq!(checkpoint.kind(), EditorDocumentKind::Prefab);
        assert_eq!(
            checkpoint.source_binding().unwrap().locator(),
            "prefabs/reference-ui.prefab.json"
        );
        assert!(checkpoint.prefab().unwrap().roots.is_empty());
    }

    #[test]
    fn prefab_patch_history_is_exact_and_scene_commands_remain_kind_bound() {
        let mut workspace = EditorWorkspace::new();
        let registry = composition_registry();
        let prefab = workspace
            .open_prefab_session(
                "ui",
                EditorDocumentSourceBinding::new("reference-ui.prefab.json"),
                PrefabAuthoringSession::new(PrefabDocument::from_ordered_roots([
                    SceneEntityRecord::new(entity_id("retry")).with_component(
                        ComponentTypeId::new("nara.transform.Transform2d"),
                        SceneComponentRecord::new(
                            ComponentSchemaVersion::ONE,
                            transform_value(0.0, 0.0),
                        ),
                    ),
                ])),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let saved_revision = workspace.prefab(prefab).unwrap().revision();

        let applied = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyPrefabPatch {
                document: prefab,
                base_revision: saved_revision,
                patch: set_transform_x_patch("retry", 12.0),
            },
        );
        assert!(applied.applied, "{:?}", applied.diagnostics);
        let accepted_revision = workspace.prefab(prefab).unwrap().revision();
        assert!(workspace.prefab(prefab).unwrap().is_dirty());
        assert_eq!(
            workspace
                .model(&registry, None)
                .document(prefab)
                .unwrap()
                .history,
            SceneAuthoringHistoryStatus {
                undo_depth: 1,
                redo_depth: 0,
            }
        );

        let scene_only = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document: prefab,
                base_revision: accepted_revision,
                patch: set_transform_x_patch("retry", 24.0),
            },
        );
        assert!(!scene_only.applied);
        assert_eq!(
            scene_only
                .diagnostics
                .iter()
                .next()
                .unwrap()
                .code()
                .as_str(),
            "tooling.workspace-document-kind-mismatch"
        );

        let stale = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyPrefabPatch {
                document: prefab,
                base_revision: saved_revision,
                patch: set_transform_x_patch("retry", 36.0),
            },
        );
        assert!(!stale.applied);
        assert_eq!(
            stale.diagnostics.iter().next().unwrap().code().as_str(),
            "tooling.workspace-stale-document-revision"
        );

        let undo = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::Undo {
                document: prefab,
                base_revision: accepted_revision,
            },
        );
        assert!(undo.applied);
        assert!(!workspace.prefab(prefab).unwrap().is_dirty());
        assert_eq!(
            workspace
                .model(&registry, None)
                .document(prefab)
                .unwrap()
                .history,
            SceneAuthoringHistoryStatus {
                undo_depth: 0,
                redo_depth: 1,
            }
        );

        let undo_revision = workspace.prefab(prefab).unwrap().revision();
        let redo = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::Redo {
                document: prefab,
                base_revision: undo_revision,
            },
        );
        assert!(redo.applied);
        assert!(workspace.prefab(prefab).unwrap().is_dirty());
        assert_eq!(
            workspace
                .prefab(prefab)
                .unwrap()
                .session()
                .document()
                .roots
                .len(),
            1
        );
    }

    #[test]
    fn selecting_missing_scene_entity_reports_workspace_diagnostic() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let document = open_scene(
            &mut workspace,
            "scene",
            scene_with_entity(entity_id("hero")),
        );

        let report = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document,
                entity: Some(entity_id("missing")),
            },
        );

        assert!(!report.applied);
        assert!(report.diagnostics.has_errors());
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == "tooling.workspace-missing-entity")
            .unwrap();
        assert_eq!(
            diagnostic_field_value(diagnostic, "target_document"),
            DiagnosticValueRef::Unsigned(document.raw())
        );
        assert_eq!(
            diagnostic_field_value(diagnostic, "entity"),
            DiagnosticValueRef::Identifier("missing")
        );
        assert!(workspace.scene(document).unwrap().selection().is_empty());
    }

    #[test]
    fn missing_requested_document_does_not_fall_back_to_active_document() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let active = open_scene(
            &mut workspace,
            "active-a",
            scene_with_entity(entity_id("hero")),
        );
        let requested = EditorDocumentId::from_raw(active.raw().saturating_add(100));

        let report = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document: requested,
                entity: Some(entity_id("target-b")),
            },
        );

        assert!(!report.applied);
        assert_eq!(report.active_document, Some(active));
        assert_eq!(report.document, Some(requested));
        let entry = report
            .diagnostics
            .iter()
            .find(|entry| entry.code().as_str() == "tooling.workspace-missing-document")
            .unwrap();
        assert_eq!(
            diagnostic_field_value(entry, "active_document"),
            DiagnosticValueRef::Unsigned(active.raw())
        );
        assert_eq!(
            diagnostic_field_value(entry, "requested_document"),
            DiagnosticValueRef::Unsigned(requested.raw())
        );
        assert_eq!(
            diagnostic_field_value(entry, "target_document"),
            DiagnosticValueRef::Unsigned(requested.raw())
        );
        assert_eq!(
            diagnostic_field_value(entry, "missing_document"),
            DiagnosticValueRef::Unsigned(requested.raw())
        );
    }

    #[test]
    fn exact_mutation_rejects_wrong_document_and_stale_revision_atomically() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let hero = entity_id("hero");
        let document = open_scene(&mut workspace, "scene", scene_with_entity(hero.clone()));
        assert!(
            workspace
                .apply_command(
                    &registry,
                    EditorWorkspaceCommand::SelectEntity {
                        document,
                        entity: Some(hero.clone()),
                    },
                )
                .applied
        );
        let base_revision = workspace.scene(document).unwrap().revision();
        let missing = EditorDocumentId::from_raw(document.raw().saturating_add(100));
        let wrong_document = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document: missing,
                base_revision,
                patch: add_entity_patch("wrong"),
            },
        );
        assert!(!wrong_document.applied);
        assert_eq!(workspace.scene(document).unwrap().revision(), base_revision);
        assert_eq!(
            workspace.scene(document).unwrap().selection().top_entity(),
            Some(&hero)
        );

        assert!(
            workspace
                .apply_command(
                    &registry,
                    EditorWorkspaceCommand::ApplyScenePatch {
                        document,
                        base_revision,
                        patch: add_entity_patch("accepted"),
                    },
                )
                .applied
        );
        let after = workspace.scene(document).unwrap();
        let after_document = after.session().document().clone();
        let after_revision = after.revision();
        let after_state = after.session().content_state();
        let after_history = after.session().history_status();
        let after_selection = after.selection().clone();
        let after_dirty = after.is_dirty();

        let stale = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document,
                base_revision,
                patch: add_entity_patch("stale"),
            },
        );
        assert!(!stale.applied);
        assert_eq!(
            stale.diagnostics.iter().next().unwrap().code().as_str(),
            "tooling.workspace-stale-document-revision"
        );
        let current = workspace.scene(document).unwrap();
        assert_eq!(current.session().document(), &after_document);
        assert_eq!(current.revision(), after_revision);
        assert_eq!(current.session().content_state(), after_state);
        assert_eq!(current.session().history_status(), after_history);
        assert_eq!(current.selection(), &after_selection);
        assert_eq!(current.is_dirty(), after_dirty);
    }

    #[test]
    fn requested_document_remains_target_when_another_document_is_active() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let active = open_scene(
            &mut workspace,
            "active-a",
            scene_with_entity(entity_id("hero-a")),
        );
        let requested = open_scene(
            &mut workspace,
            "target-b",
            scene_with_entity(entity_id("hero-b")),
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SetActiveDocument { document: active },
        );

        let report = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document: requested,
                entity: Some(entity_id("missing-in-b")),
            },
        );

        assert!(!report.applied);
        assert_eq!(report.active_document, Some(active));
        assert_eq!(report.document, Some(requested));
        let entry = report
            .diagnostics
            .iter()
            .find(|entry| entry.code().as_str() == "tooling.workspace-missing-entity")
            .unwrap();
        assert_eq!(
            diagnostic_field_value(entry, "active_document"),
            DiagnosticValueRef::Unsigned(active.raw())
        );
        assert_eq!(
            diagnostic_field_value(entry, "requested_document"),
            DiagnosticValueRef::Unsigned(requested.raw())
        );
        assert_eq!(
            diagnostic_field_value(entry, "target_document"),
            DiagnosticValueRef::Unsigned(requested.raw())
        );
    }

    #[test]
    fn undo_requires_the_exact_document_and_current_revision() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let first = open_scene(&mut workspace, "first", SceneDocument::default());
        let second = open_scene(&mut workspace, "second", SceneDocument::default());

        apply_patch(
            &mut workspace,
            &registry,
            first,
            add_entity_patch("first_entity"),
        );
        let stale_second = workspace.scene(second).unwrap().revision();
        apply_patch(
            &mut workspace,
            &registry,
            second,
            add_entity_patch("second_entity"),
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SetActiveDocument { document: first },
        );

        let stale = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::Undo {
                document: second,
                base_revision: stale_second,
            },
        );
        assert!(!stale.applied);
        assert!(document_has_entity(
            workspace.scene(second).unwrap().session().document(),
            &entity_id("second_entity")
        ));

        assert!(undo(&mut workspace, &registry, second).applied);

        assert!(document_has_entity(
            workspace.scene(first).unwrap().session().document(),
            &entity_id("first_entity")
        ));
        assert!(!document_has_entity(
            workspace.scene(second).unwrap().session().document(),
            &entity_id("second_entity")
        ));
    }

    #[test]
    fn inspector_move_and_workspace_history_preserve_exact_root_order() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let first = entity_id("first");
        let middle = entity_id("middle");
        let last = entity_id("last");
        let original = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(first.clone()),
            SceneEntityRecord::new(middle.clone()),
            SceneEntityRecord::new(last.clone()),
        ]);
        let document = open_scene(&mut workspace, "ordered", original.clone());

        let moved = apply_inspector(
            &mut workspace,
            &registry,
            document,
            SceneInspectorCommand::MoveEntity {
                entity: last.clone(),
                to: ScenePlacement::first(ScenePlacementOwner::Root),
            },
        );

        assert!(moved.applied, "{:?}", moved.diagnostics);
        let slot = workspace.scene(document).unwrap();
        assert_eq!(
            slot.session().document().roots,
            [last.clone(), first, middle]
        );
        assert_eq!(slot.session().history_status().undo_depth, 1);

        assert!(undo(&mut workspace, &registry, document).applied);
        assert_eq!(
            workspace.scene(document).unwrap().session().document(),
            &original
        );

        assert!(redo(&mut workspace, &registry, document).applied);
        assert_eq!(
            workspace
                .scene(document)
                .unwrap()
                .session()
                .document()
                .roots,
            [last, entity_id("first"), entity_id("middle")]
        );
    }

    #[test]
    fn external_reload_tracks_pending_and_dirty_conflict() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let document = open_scene(&mut workspace, "scene", SceneDocument::default());

        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::MarkExternalChanged { document },
        );
        assert_eq!(
            workspace.scene(document).unwrap().external_reload(),
            EditorExternalReloadState::Pending
        );

        let base_revision = workspace.scene(document).unwrap().revision();
        workspace
            .reload_external_session(
                document,
                base_revision,
                SceneAuthoringSession::new(scene_with_entity(entity_id("fresh"))),
            )
            .unwrap();
        assert!(document_has_entity(
            workspace.scene(document).unwrap().session().document(),
            &entity_id("fresh")
        ));
        assert_eq!(
            workspace.scene(document).unwrap().external_reload(),
            EditorExternalReloadState::Clean
        );

        apply_patch(
            &mut workspace,
            &registry,
            document,
            add_entity_patch("dirty"),
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::MarkExternalChanged { document },
        );
        assert_eq!(
            workspace.scene(document).unwrap().external_reload(),
            EditorExternalReloadState::Conflict
        );

        let replacement = SceneAuthoringSession::new(scene_with_entity(entity_id("rejected")));
        let replacement_revision = replacement.revision();
        let base_revision = workspace.scene(document).unwrap().revision();
        let error = workspace
            .reload_external_session(document, base_revision, replacement)
            .unwrap_err();
        assert!(
            error
                .report()
                .diagnostics
                .iter()
                .any(|entry| entry.code().as_str() == "tooling.workspace-reload-conflict")
        );
        let replacement = error.into_session();
        assert_eq!(replacement.revision(), replacement_revision);
        let slot = workspace.scene(document).unwrap();
        assert!(document_has_entity(
            slot.session().document(),
            &entity_id("fresh")
        ));
        assert!(!document_has_entity(
            slot.session().document(),
            &entity_id("rejected")
        ));
    }

    #[test]
    fn opening_a_session_preserves_its_authoring_source_identity() {
        let mut workspace = EditorWorkspace::new();
        let session = SceneAuthoringSession::new(scene_with_entity(entity_id("hero")));
        let source_revision = session.revision();

        let document = workspace
            .open_scene_session("scene", session)
            .unwrap()
            .opened_document
            .unwrap();

        assert_eq!(
            workspace.scene(document).unwrap().session().revision(),
            source_revision
        );
    }

    #[test]
    fn reloading_from_a_session_adopts_the_new_publication_identity() {
        let mut workspace = EditorWorkspace::new();
        let document = open_scene(&mut workspace, "scene", SceneDocument::default());
        let original_revision = workspace.scene(document).unwrap().revision();
        let replacement = SceneAuthoringSession::new(scene_with_entity(entity_id("replacement")));
        let replacement_revision = replacement.revision();
        assert_ne!(
            replacement_revision.source_id(),
            original_revision.source_id()
        );

        let base_revision = workspace.scene(document).unwrap().revision();
        let report = workspace
            .reload_external_session(document, base_revision, replacement)
            .unwrap();

        assert!(report.applied);
        let slot = workspace.scene(document).unwrap();
        assert_eq!(slot.revision(), replacement_revision);
        assert!(document_has_entity(
            slot.session().document(),
            &entity_id("replacement")
        ));
        assert!(!slot.is_dirty());
        assert_eq!(slot.external_reload(), EditorExternalReloadState::Clean);
    }

    #[test]
    fn rejected_session_publication_returns_the_original_session() {
        let mut workspace = EditorWorkspace::new();
        let missing = EditorDocumentId::from_raw(99);
        let replacement = SceneAuthoringSession::new(scene_with_entity(entity_id("replacement")));
        let replacement_revision = replacement.revision();

        let error = workspace
            .reload_external_session(missing, replacement_revision, replacement)
            .unwrap_err();
        assert!(
            error
                .report()
                .diagnostics
                .iter()
                .any(|entry| entry.code().as_str() == "tooling.workspace-missing-document")
        );
        let replacement = error.into_session();
        assert_eq!(replacement.revision(), replacement_revision);
        assert!(document_has_entity(
            replacement.document(),
            &entity_id("replacement")
        ));
        assert!(workspace.is_empty());
    }

    #[test]
    fn attached_session_cannot_be_published_without_its_live_world() {
        let registry = frozen_empty_registry();
        let mut session = SceneAuthoringSession::new(SceneDocument::default());
        let mut world = nara_ecs::World::new();
        assert!(
            session
                .sync_world(
                    &mut world,
                    &registry,
                    &nara_scene::SourceDocumentKey::named(
                        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
                        "workspace-attached-session",
                        nara_scene::SourceDocumentKind::Scene
                    )
                    .unwrap(),
                    &nara_scene::SceneOccurrenceKey::new(
                        "tooling.workspace.tests",
                        "attached-session"
                    )
                    .unwrap()
                )
                .unwrap()
                .synced
        );
        assert!(session.live_instance().is_some());
        let mut workspace = EditorWorkspace::new();

        let error = workspace.open_scene_session("scene", session).unwrap_err();

        assert!(
            error
                .report()
                .diagnostics
                .iter()
                .any(|entry| entry.code().as_str() == "tooling.workspace-session-attached")
        );
        assert!(error.into_session().live_instance().is_some());
        assert!(workspace.is_empty());
    }

    #[test]
    fn degraded_authoring_field_gate_preserves_manifest_and_is_failure_atomic() {
        let resolved =
            nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER)
                .unwrap();
        let owner =
            PersistentComponentSchemaOwnerId::try_new(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str())
                .unwrap();
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner.clone(),
            resolved.current().catalog().clone(),
            resolved
                .predecessor()
                .map(|record| record.catalog().clone()),
        )])
        .unwrap();
        let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_trusted_provider(
            resolved.contribution_receipt(),
            "nara.scene.first-party-authoring-adapter",
            1,
            1,
            [ComponentTypeId::new("nara.scene.Name")],
        )
        .unwrap();
        let entity = entity_id("player");
        let name = ComponentTypeId::new("nara.scene.Name");
        let visibility = ComponentTypeId::new("nara.scene.Visibility");
        let source =
            AuthoringDocumentSource::new(AssetRef::path("scenes/degraded.scene.json").unwrap());
        let published = SceneDocumentCandidate::decode_json_bytes(
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
                .with_component(
                    name.clone(),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::String("Player".to_owned()),
                    )
                    .with_owner(owner.clone()),
                )
                .with_component(
                    visibility.clone(),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::String("visible".to_owned()),
                    )
                    .with_owner(owner),
                )])
            .to_json_string()
            .unwrap()
            .as_bytes(),
        )
        .unwrap()
        .admit_authoring(source, &bundle, &[adapter])
        .unwrap()
        .publish();
        let expected_manifest = published.unavailable_manifest().clone();

        let mut workspace = EditorWorkspace::new();
        let document = workspace
            .open_authoring_scene_document(
                "degraded",
                EditorDocumentSourceBinding::new("scenes/degraded.scene.json"),
                published,
            )
            .unwrap()
            .opened_document
            .unwrap();
        assert_eq!(
            workspace
                .authoring_model()
                .document(document)
                .unwrap()
                .authoring_readiness,
            Some(AuthoringDocumentReadiness::Degraded)
        );
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document,
                    entity: Some(entity.clone()),
                })
                .applied
        );
        let inspector = &workspace
            .authoring_model()
            .active_scene
            .unwrap()
            .editor
            .inspector;
        let name_view = inspector
            .selected_entity_view
            .as_ref()
            .unwrap()
            .components
            .iter()
            .find(|component| component.component == name)
            .unwrap();
        assert!(name_view.access.is_editable());
        assert_eq!(name_view.fields.len(), 1);
        assert_eq!(name_view.fields[0].id, ComponentFieldId::new("value"));
        assert_eq!(
            name_view.fields[0].value,
            Some(ComponentValue::String("Player".to_owned()))
        );
        let visibility_view = inspector
            .selected_entity_view
            .as_ref()
            .unwrap()
            .components
            .iter()
            .find(|component| component.component == visibility)
            .unwrap();
        assert_eq!(
            visibility_view.access.read_only_reason(),
            Some(crate::SceneInspectorReadOnlyReason::KnownUnbound)
        );
        assert_eq!(
            visibility_view.document_version,
            ComponentSchemaVersion::ONE
        );
        assert!(visibility_view.fields.is_empty());

        let edit_base_revision = workspace.document_revision(document).unwrap();
        let edit =
            workspace.apply_authoring_command(EditorWorkspaceCommand::ApplyInspectorCommand {
                document,
                base_revision: edit_base_revision,
                command: SceneInspectorCommand::SetField {
                    entity: entity.clone(),
                    component: name.clone(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("value"),
                    value: ComponentValue::String("Renamed".to_owned()),
                },
            });
        assert!(edit.applied, "{:#?}", edit.diagnostics);
        assert_ne!(edit.revision, Some(edit_base_revision));

        let undo = workspace.apply_authoring_command(EditorWorkspaceCommand::Undo {
            document,
            base_revision: edit.revision.unwrap(),
        });
        assert!(undo.applied, "{:#?}", undo.diagnostics);
        assert_eq!(
            workspace
                .scene(document)
                .unwrap()
                .unavailable_record_manifest(),
            Some(&expected_manifest)
        );
        let redo = workspace.apply_authoring_command(EditorWorkspaceCommand::Redo {
            document,
            base_revision: undo.revision.unwrap(),
        });
        assert!(redo.applied, "{:#?}", redo.diagnostics);
        assert_eq!(
            workspace
                .scene(document)
                .unwrap()
                .unavailable_record_manifest(),
            Some(&expected_manifest)
        );

        let slot = workspace.scene(document).unwrap();
        let accepted_revision = slot.revision();
        let accepted_content_state = slot.session().content_state();
        let accepted_history = slot.session().history_status();
        let accepted_selection = slot.selection().clone();
        let accepted_document = slot.session().document().clone();
        assert!(slot.is_dirty());
        assert_eq!(accepted_history.undo_depth, 1);
        assert_eq!(slot.unavailable_record_manifest(), Some(&expected_manifest));

        let unavailable_edit =
            workspace.apply_authoring_command(EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                document,
                base_revision: accepted_revision,
                edit: AuthoringFieldEdit::single(
                    entity.clone(),
                    visibility,
                    ComponentSchemaVersion::ONE,
                    ComponentFieldId::new("value"),
                    ComponentValue::String("hidden".to_owned()),
                )
                .unwrap(),
            });
        assert!(!unavailable_edit.applied);
        let raw_patch =
            workspace.apply_authoring_command(EditorWorkspaceCommand::ApplyScenePatch {
                document,
                base_revision: accepted_revision,
                patch: ScenePatchDocument::new([ScenePatchOperation::RemoveComponent {
                    entity: entity.clone(),
                    component: ComponentTypeId::new("nara.future.Missing"),
                }]),
            });
        assert!(!raw_patch.applied);
        assert!(
            raw_patch
                .diagnostics
                .iter()
                .any(|entry| { entry.code().as_str() == "scene.authoring-structure-unavailable" })
        );
        let structure = workspace.apply_authoring_command(EditorWorkspaceCommand::ComposeScene {
            document,
            base_revision: accepted_revision,
            command: SceneCompositionCommand::RemoveEntity { entity },
        });
        assert!(!structure.applied);
        assert!(
            structure
                .diagnostics
                .iter()
                .any(|entry| { entry.code().as_str() == "scene.authoring-structure-unavailable" })
        );

        let slot = workspace.scene(document).unwrap();
        assert_eq!(slot.revision(), accepted_revision);
        assert_eq!(slot.session().content_state(), accepted_content_state);
        assert_eq!(slot.session().history_status(), accepted_history);
        assert_eq!(slot.selection(), &accepted_selection);
        assert_eq!(slot.session().document(), &accepted_document);
        assert!(slot.is_dirty());
        assert_eq!(slot.unavailable_record_manifest(), Some(&expected_manifest));
    }

    fn diagnostic_field_value<'a>(
        entry: &'a nara_diagnostic::Diagnostic,
        key: &str,
    ) -> DiagnosticValueRef<'a> {
        entry
            .fields()
            .iter()
            .find(|field| field.key().as_str() == key)
            .unwrap()
            .value()
    }

    fn apply_patch(
        workspace: &mut EditorWorkspace,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        patch: ScenePatchDocument,
    ) -> EditorWorkspaceCommandReport {
        let base_revision = workspace.scene(document).unwrap().revision();
        workspace.apply_command(
            registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document,
                base_revision,
                patch,
            },
        )
    }

    fn apply_inspector(
        workspace: &mut EditorWorkspace,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        command: SceneInspectorCommand,
    ) -> EditorWorkspaceCommandReport {
        let base_revision = workspace.scene(document).unwrap().revision();
        workspace.apply_command(
            registry,
            EditorWorkspaceCommand::ApplyInspectorCommand {
                document,
                base_revision,
                command,
            },
        )
    }

    fn compose(
        workspace: &mut EditorWorkspace,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        command: SceneCompositionCommand,
    ) -> EditorWorkspaceCommandReport {
        let base_revision = workspace.scene(document).unwrap().revision();
        workspace.apply_command(
            registry,
            EditorWorkspaceCommand::ComposeScene {
                document,
                base_revision,
                command,
            },
        )
    }

    fn compose_prefab(
        workspace: &mut EditorWorkspace,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
        command: PrefabCompositionCommand,
    ) -> EditorWorkspaceCommandReport {
        let base_revision = workspace.prefab(document).unwrap().revision();
        workspace.apply_command(
            registry,
            EditorWorkspaceCommand::ComposePrefab {
                document,
                base_revision,
                command,
            },
        )
    }

    fn undo(
        workspace: &mut EditorWorkspace,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
    ) -> EditorWorkspaceCommandReport {
        let base_revision = workspace.scene(document).unwrap().revision();
        workspace.apply_command(
            registry,
            EditorWorkspaceCommand::Undo {
                document,
                base_revision,
            },
        )
    }

    fn redo(
        workspace: &mut EditorWorkspace,
        registry: &ComponentRegistry,
        document: EditorDocumentId,
    ) -> EditorWorkspaceCommandReport {
        let base_revision = workspace.scene(document).unwrap().revision();
        workspace.apply_command(
            registry,
            EditorWorkspaceCommand::Redo {
                document,
                base_revision,
            },
        )
    }

    fn frozen_empty_registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        registry
    }

    fn authoring_basis_bundle(alias: &str) -> AuthoringSchemaBundle {
        AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("test.basis").unwrap(),
            ComponentSchemaCatalog {
                components: vec![ComponentSchema::new(
                    ComponentTypeId::new("test.basis.Component"),
                    alias,
                    ComponentSchemaVersion::ONE,
                )],
                ..ComponentSchemaCatalog::default()
            },
            None,
        )])
        .unwrap()
    }

    fn published_basis_scene(bundle: &AuthoringSchemaBundle) -> PublishedAuthoringSceneDocument {
        SceneDocumentCandidate::decode_json_bytes(
            SceneDocument::default()
                .to_json_string()
                .unwrap()
                .as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scenes/basis.scene.json").unwrap()),
            bundle,
            &[],
        )
        .unwrap()
        .publish()
    }

    fn composition_registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        nara_transform::register_transform_components(&mut registry).unwrap();
        nara_sprite::register_sprite_components(&mut registry).unwrap();
        registry.freeze().unwrap();
        registry
    }

    fn open_scene(
        workspace: &mut EditorWorkspace,
        title: &str,
        document: SceneDocument,
    ) -> EditorDocumentId {
        workspace
            .open_scene_session(title.to_owned(), SceneAuthoringSession::new(document))
            .unwrap()
            .opened_document
            .unwrap()
    }

    fn open_prefab(
        workspace: &mut EditorWorkspace,
        title: &str,
        document: PrefabDocument,
    ) -> EditorDocumentId {
        workspace
            .open_prefab_session(
                title.to_owned(),
                EditorDocumentSourceBinding::new(format!("prefabs/{title}.prefab.json")),
                PrefabAuthoringSession::new(document),
            )
            .unwrap()
            .opened_document
            .unwrap()
    }

    fn scene_with_entity(entity: SceneEntityId) -> SceneDocument {
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity)])
    }

    fn add_entity_patch(id: &str) -> ScenePatchDocument {
        ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(entity_id(id)),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }])
    }

    fn set_transform_x_patch(entity: &str, value: f64) -> ScenePatchDocument {
        ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: entity_id(entity),
            component: ComponentTypeId::new("nara.transform.Transform2d"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("translation.x"),
            value: ComponentValue::f64(value).unwrap(),
        }])
    }

    fn transform_value(x: f64, y: f64) -> ComponentValue {
        ComponentValue::map([
            (
                "translation",
                ComponentValue::map([
                    ("x", ComponentValue::f64(x).unwrap()),
                    ("y", ComponentValue::f64(y).unwrap()),
                ]),
            ),
            ("rotation", ComponentValue::f64(0.0).unwrap()),
            (
                "scale",
                ComponentValue::map([
                    ("x", ComponentValue::f64(1.0).unwrap()),
                    ("y", ComponentValue::f64(1.0).unwrap()),
                ]),
            ),
        ])
    }

    fn entity_id(id: &str) -> SceneEntityId {
        SceneEntityId::new(id).unwrap()
    }
}
