use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_diagnostic::DiagnosticReport;
use nara_ecs::Resource;
use nara_fs::{
    FileIdentity, PublicationAtomicity, PublicationIdentityEvidence, ReplaceReceipt, StageStatus,
};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    SceneAuthoringRevision, SceneAuthoringSession, SceneDocument, SceneEntityId,
    ScenePatchDocument, ScenePatchReport,
};

use crate::diagnostic;
use crate::inspector::{SceneInspectorCommand, SceneInspectorCommandReport};
use crate::play::{SceneEditorModel, SceneEditorState};
use crate::snapshot::WorldIdentitySnapshot;

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
    revision: SceneAuthoringRevision,
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
}

impl EditorPersistenceCheckpoint {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
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

#[derive(Debug, Default, Resource)]
pub struct EditorWorkspace {
    persistence_identity: Arc<EditorWorkspacePersistenceIdentity>,
    next_document_id: u64,
    active_document: Option<EditorDocumentId>,
    scenes: BTreeMap<EditorDocumentId, EditorSceneSlot>,
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
        self.scenes.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.scenes.len()
    }

    #[must_use]
    pub fn scene(&self, document: EditorDocumentId) -> Option<&EditorSceneSlot> {
        self.scenes.get(&document)
    }

    #[must_use]
    pub fn active_scene(&self) -> Option<&EditorSceneSlot> {
        self.active_document.and_then(|id| self.scenes.get(&id))
    }

    fn apply_persistence_checkpoint(
        &mut self,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        digest: EditorDocumentDigest,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(slot) = self.scenes.get_mut(&document) else {
            return workspace_document_error_report(context, document);
        };
        let current = slot.revision();
        let saved = slot.saved_revision();
        let revision_matches_source = revision.source_id() == current.source_id();
        let revision_not_ahead = revision.generation() <= current.generation();
        let revision_not_regressed = revision.source_id() == saved.source_id()
            && revision.generation() >= saved.generation();
        if !revision_matches_source || !revision_not_ahead || !revision_not_regressed {
            return workspace_error_report(
                context,
                "tooling.workspace-persistence-checkpoint-mismatch",
                "persistence checkpoint does not match the open document revision",
            );
        }
        slot.apply_persistence_checkpoint(revision, digest);
        report_for_slot(document, active_document, slot)
    }

    fn bind_opened_source_digest(
        &mut self,
        document: EditorDocumentId,
        digest: EditorDocumentDigest,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, Some(document));
        let Some(slot) = self.scenes.get_mut(&document) else {
            return workspace_document_error_report(context, document);
        };
        slot.saved_digest = Some(digest);
        report_for_slot(document, active_document, slot)
    }

    pub fn open_scene_session(
        &mut self,
        title: impl Into<String>,
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
        let slot = EditorSceneSlot::from_session(title, session);
        self.scenes.insert(document_id, slot);
        self.active_document = Some(document_id);
        let dirty = self
            .scenes
            .get(&document_id)
            .is_some_and(EditorSceneSlot::is_dirty);
        Ok(EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document_id),
            opened_document: Some(document_id),
            active_document: self.active_document,
            dirty: Some(dirty),
            revision: self.scenes.get(&document_id).map(|slot| slot.revision()),
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        })
    }

    pub fn apply_command(
        &mut self,
        registry: &ComponentRegistry,
        command: EditorWorkspaceCommand,
    ) -> EditorWorkspaceCommandReport {
        match command {
            EditorWorkspaceCommand::CloseScene { document } => self.close_scene(document),
            EditorWorkspaceCommand::SetActiveScene { document } => self.set_active_scene(document),
            EditorWorkspaceCommand::SelectEntity { document, entity } => {
                self.select_entity(document, entity)
            }
            EditorWorkspaceCommand::ApplyInspectorCommand { document, command } => {
                self.apply_inspector_command(registry, document, command)
            }
            EditorWorkspaceCommand::ApplyScenePatch { document, patch } => {
                self.apply_scene_patch(registry, document, patch)
            }
            EditorWorkspaceCommand::Undo { document } => self.undo(registry, document),
            EditorWorkspaceCommand::Redo { document } => self.redo(registry, document),
            EditorWorkspaceCommand::MarkExternalChanged { document } => {
                self.mark_external_changed(document)
            }
        }
    }

    #[must_use]
    pub fn model(
        &mut self,
        registry: &ComponentRegistry,
        edit_world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> EditorWorkspaceModel {
        let active_scene = self.active_document.and_then(|document| {
            self.scenes
                .get_mut(&document)
                .map(|slot| slot.model(document, registry, edit_world_snapshot))
        });

        EditorWorkspaceModel {
            active_document: self.active_document,
            scenes: self
                .scenes
                .iter()
                .map(|(document, slot)| EditorSceneTabModel {
                    document: *document,
                    title: slot.title.clone(),
                    dirty: slot.is_dirty(),
                    external_reload: slot.external_reload,
                    active: Some(*document) == self.active_document,
                })
                .collect(),
            active_scene,
            diagnostics: DiagnosticReport::default(),
        }
    }

    fn close_scene(&mut self, document: Option<EditorDocumentId>) -> EditorWorkspaceCommandReport {
        let context = WorkspaceDocumentContext::new(self.active_document, document);
        let Some(document) = context.target else {
            return workspace_error_report(
                context,
                "tooling.workspace-no-active-document",
                "workspace command requires an active document",
            );
        };
        let Some(slot) = self.scenes.get(&document) else {
            return workspace_document_error_report(context, document);
        };
        if slot.is_dirty() {
            return workspace_error_report(
                context,
                "tooling.workspace-dirty-close-decision-required",
                "closing a dirty document requires an explicit save, discard, or cancel decision",
            );
        }
        self.remove_scene(document)
    }

    /// Removes one dirty document after a concrete Host has recorded an explicit discard choice.
    ///
    /// This is Host plumbing rather than an author command. Product paths must present the
    /// Save/Discard/Cancel decision before invoking it.
    #[doc(hidden)]
    pub fn __discard_scene(&mut self, document: EditorDocumentId) -> EditorWorkspaceCommandReport {
        let context = WorkspaceDocumentContext::new(self.active_document, Some(document));
        if !self.scenes.contains_key(&document) {
            return workspace_document_error_report(context, document);
        }
        self.remove_scene(document)
    }

    fn remove_scene(&mut self, document: EditorDocumentId) -> EditorWorkspaceCommandReport {
        let removed = self.scenes.remove(&document);
        debug_assert!(removed.is_some(), "the document was checked before removal");
        if self.active_document == Some(document) {
            self.active_document = self.scenes.keys().next().copied();
        }
        EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document),
            active_document: self.active_document,
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        }
    }

    fn set_active_scene(
        &mut self,
        document: Option<EditorDocumentId>,
    ) -> EditorWorkspaceCommandReport {
        let context = WorkspaceDocumentContext::new(self.active_document, document);
        let Some(document) = context.target else {
            return workspace_error_report(
                context,
                "tooling.workspace-no-active-document",
                "workspace has no active document",
            );
        };
        if !self.scenes.contains_key(&document) {
            return workspace_document_error_report(context, document);
        }
        self.active_document = Some(document);
        EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document),
            active_document: self.active_document,
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        }
    }

    fn select_entity(
        &mut self,
        document: Option<EditorDocumentId>,
        entity: Option<SceneEntityId>,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        if let Some(entity) = &entity
            && !document_has_entity(slot.session.document(), entity)
        {
            return workspace_entity_error_report(
                context,
                document,
                "tooling.workspace-missing-entity",
                "workspace selection targets an entity that is not in the scene document",
                entity,
            );
        }
        slot.selection.select_entity(entity);
        slot.sync_editor_selection();
        report_for_slot(document, active_document, slot)
    }

    fn apply_inspector_command(
        &mut self,
        registry: &ComponentRegistry,
        document: Option<EditorDocumentId>,
        command: SceneInspectorCommand,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        slot.sync_editor_selection();
        let inspector_report =
            slot.editor
                .apply_inspector_command(&mut slot.session, registry, command);
        if inspector_report.applied {
            slot.selection
                .select_entity(inspector_report.selected_entity.clone());
            slot.selection.retain_existing(slot.session.document());
            slot.sync_editor_selection();
        }
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = inspector_report.applied;
        report.inspector_report = Some(inspector_report.clone());
        let _ = report.diagnostics.extend(inspector_report.diagnostics);
        report
    }

    fn apply_scene_patch(
        &mut self,
        registry: &ComponentRegistry,
        document: Option<EditorDocumentId>,
        patch: ScenePatchDocument,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        let patch_report = slot.session.apply_patch(&patch, registry);
        if patch_report.applied {
            slot.selection.retain_existing(slot.session.document());
            slot.sync_editor_selection();
        }
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = patch_report.applied;
        report.patch_report = Some(patch_report.clone());
        let _ = report.diagnostics.extend(patch_report.diagnostics);
        report
    }

    fn undo(
        &mut self,
        registry: &ComponentRegistry,
        document: Option<EditorDocumentId>,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        let patch_report = slot.session.undo(registry);
        if patch_report.applied {
            slot.selection.retain_existing(slot.session.document());
            slot.sync_editor_selection();
        }
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = patch_report.applied;
        report.patch_report = Some(patch_report.clone());
        let _ = report.diagnostics.extend(patch_report.diagnostics);
        report
    }

    fn redo(
        &mut self,
        registry: &ComponentRegistry,
        document: Option<EditorDocumentId>,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        let patch_report = slot.session.redo(registry);
        if patch_report.applied {
            slot.selection.retain_existing(slot.session.document());
            slot.sync_editor_selection();
        }
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = patch_report.applied;
        report.patch_report = Some(patch_report.clone());
        let _ = report.diagnostics.extend(patch_report.diagnostics);
        report
    }

    fn mark_external_changed(
        &mut self,
        document: Option<EditorDocumentId>,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        slot.external_reload = if slot.is_dirty() {
            EditorExternalReloadState::Conflict
        } else {
            EditorExternalReloadState::Pending
        };
        report_for_slot(document, active_document, slot)
    }

    pub fn reload_external_session(
        &mut self,
        document: Option<EditorDocumentId>,
        session: SceneAuthoringSession,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return Err(EditorSceneSessionPublicationError::new(
                workspace_resolution_error_report(context),
                session,
            ));
        };
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
        document: Option<EditorDocumentId>,
        session: SceneAuthoringSession,
        digest: EditorDocumentDigest,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return Err(EditorSceneSessionPublicationError::new(
                workspace_resolution_error_report(context),
                session,
            ));
        };
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
        slot.saved_digest = Some(digest);
        slot.external_reload = EditorExternalReloadState::Clean;
        Ok(report_for_slot(document, active_document, slot))
    }

    fn allocate_document_id(&mut self) -> EditorDocumentId {
        self.next_document_id = self.next_document_id.saturating_add(1);
        EditorDocumentId::from_raw(self.next_document_id)
    }

    fn resolve_document(&self, document: Option<EditorDocumentId>) -> Option<EditorDocumentId> {
        document.or(self.active_document)
    }

    fn resolve_scene_mut(
        &mut self,
        document: Option<EditorDocumentId>,
    ) -> Option<(EditorDocumentId, &mut EditorSceneSlot)> {
        let document = self.resolve_document(document)?;
        let slot = self.scenes.get_mut(&document)?;
        Some((document, slot))
    }
}

#[derive(Debug)]
pub struct EditorSceneSlot {
    title: String,
    session: SceneAuthoringSession,
    editor: SceneEditorState,
    selection: EditorSelectionSet,
    saved_revision: SceneAuthoringRevision,
    saved_digest: Option<EditorDocumentDigest>,
    external_reload: EditorExternalReloadState,
}

impl EditorSceneSlot {
    fn from_session(title: impl Into<String>, session: SceneAuthoringSession) -> Self {
        let saved_revision = session.revision();
        Self {
            title: title.into(),
            session,
            editor: SceneEditorState::new(),
            selection: EditorSelectionSet::default(),
            saved_revision,
            saved_digest: None,
            external_reload: EditorExternalReloadState::Clean,
        }
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
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
    pub const fn saved_digest(&self) -> Option<EditorDocumentDigest> {
        self.saved_digest
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.session.revision() != self.saved_revision || self.session.source_upgrade_required()
    }

    #[must_use]
    pub fn external_reload(&self) -> EditorExternalReloadState {
        self.external_reload
    }

    fn apply_persistence_checkpoint(
        &mut self,
        revision: SceneAuthoringRevision,
        digest: EditorDocumentDigest,
    ) {
        self.saved_revision = revision;
        self.saved_digest = Some(digest);
        self.session.acknowledge_source_saved();
        if self.external_reload != EditorExternalReloadState::Conflict {
            self.external_reload = EditorExternalReloadState::Clean;
        }
    }

    fn model(
        &mut self,
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
            title: self.title.clone(),
            dirty: self.is_dirty(),
            revision: self.session.revision(),
            saved_revision: self.saved_revision,
            external_reload: self.external_reload,
            selection: self.selection.clone(),
            editor,
        }
    }

    fn sync_editor_selection(&mut self) {
        self.editor
            .inspector_mut()
            .select_entity(self.selection.top_entity().cloned());
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
        let revision = workspace.scene(document)?.revision();
        Some(EditorPersistenceCheckpoint {
            identity: Arc::clone(&self.identity),
            document,
            revision,
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
        workspace.apply_persistence_checkpoint(checkpoint.document, checkpoint.revision, digest)
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
        document: Option<EditorDocumentId>,
        session: SceneAuthoringSession,
        digest: EditorDocumentDigest,
    ) -> Result<EditorWorkspaceCommandReport, EditorSceneSessionPublicationError> {
        if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            let context = WorkspaceDocumentContext::new(workspace.active_document, document);
            return Err(EditorSceneSessionPublicationError::new(
                workspace_error_report(
                    context,
                    "tooling.workspace-persistence-authority-mismatch",
                    "reopened session does not belong to the hosted workspace authority",
                ),
                session,
            ));
        }
        workspace.publish_reopened_session(document, session, digest)
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExternalReloadState {
    Clean,
    Pending,
    Conflict,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorWorkspaceCommand {
    CloseScene {
        document: Option<EditorDocumentId>,
    },
    SetActiveScene {
        document: Option<EditorDocumentId>,
    },
    SelectEntity {
        document: Option<EditorDocumentId>,
        entity: Option<SceneEntityId>,
    },
    ApplyInspectorCommand {
        document: Option<EditorDocumentId>,
        command: SceneInspectorCommand,
    },
    ApplyScenePatch {
        document: Option<EditorDocumentId>,
        patch: ScenePatchDocument,
    },
    Undo {
        document: Option<EditorDocumentId>,
    },
    Redo {
        document: Option<EditorDocumentId>,
    },
    MarkExternalChanged {
        document: Option<EditorDocumentId>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct EditorWorkspaceModel {
    pub active_document: Option<EditorDocumentId>,
    pub scenes: Vec<EditorSceneTabModel>,
    pub active_scene: Option<EditorSceneModel>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSceneTabModel {
    pub document: EditorDocumentId,
    pub title: String,
    pub dirty: bool,
    pub external_reload: EditorExternalReloadState,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorSceneModel {
    pub document: EditorDocumentId,
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

fn workspace_resolution_error_report(
    context: WorkspaceDocumentContext,
) -> EditorWorkspaceCommandReport {
    match context.target {
        Some(document) => workspace_document_error_report(context, document),
        None => workspace_error_report(
            context,
            "tooling.workspace-no-active-document",
            "workspace command requires an active document",
        ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use nara_app::App;
    use nara_diagnostic::DiagnosticValueRef;
    use nara_reflect::ComponentRegistry;
    use nara_scene::{SceneEntityRecord, ScenePatchOperation};

    use crate::ToolingPlugin;

    #[test]
    fn dirty_close_requires_an_explicit_decision() {
        let registry = frozen_empty_registry();
        let mut workspace = EditorWorkspace::new();
        let opened = workspace
            .open_scene_session("main", SceneAuthoringSession::new(SceneDocument::default()))
            .unwrap();
        let document = opened.opened_document.unwrap();

        let edited = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("dirty"),
            },
        );
        assert!(edited.applied);

        let close = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::CloseScene {
                document: Some(document),
            },
        );
        assert!(!close.applied);
        assert_eq!(workspace.len(), 1);
        assert_eq!(workspace.active_document(), Some(document));
        assert_eq!(
            close.diagnostics.iter().next().unwrap().code().as_str(),
            "tooling.workspace-dirty-close-decision-required"
        );

        let discarded = workspace.__discard_scene(document);
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
            workspace
                .apply_command(
                    &registry,
                    EditorWorkspaceCommand::ApplyScenePatch {
                        document: Some(document),
                        patch: add_entity_patch("captured"),
                    },
                )
                .applied
        );
        let captured_revision = workspace.scene(document).unwrap().revision();
        let accepted_checkpoint = authority.capture(&workspace, document).unwrap();
        assert!(
            workspace
                .apply_command(
                    &registry,
                    EditorWorkspaceCommand::ApplyScenePatch {
                        document: Some(document),
                        patch: add_entity_patch("later"),
                    },
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
                document: Some(first),
                entity: Some(hero.clone()),
            },
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document: Some(second),
                entity: Some(enemy.clone()),
            },
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(first),
                patch: add_entity_patch("prop"),
            },
        );

        let first_slot = workspace.scene(first).unwrap();
        let second_slot = workspace.scene(second).unwrap();
        assert_eq!(first_slot.selection().top_entity(), Some(&hero));
        assert_eq!(second_slot.selection().top_entity(), Some(&enemy));
        assert!(first_slot.is_dirty());
        assert!(!second_slot.is_dirty());
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
                document: Some(document),
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
                document: Some(requested),
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
            EditorWorkspaceCommand::SetActiveScene {
                document: Some(active),
            },
        );

        let report = workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SelectEntity {
                document: Some(requested),
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
    fn undo_targets_active_document_only() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let first = open_scene(&mut workspace, "first", SceneDocument::default());
        let second = open_scene(&mut workspace, "second", SceneDocument::default());

        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(first),
                patch: add_entity_patch("first_entity"),
            },
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(second),
                patch: add_entity_patch("second_entity"),
            },
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::SetActiveScene {
                document: Some(second),
            },
        );
        workspace.apply_command(&registry, EditorWorkspaceCommand::Undo { document: None });

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
    fn external_reload_tracks_pending_and_dirty_conflict() {
        let mut workspace = EditorWorkspace::new();
        let registry = frozen_empty_registry();
        let document = open_scene(&mut workspace, "scene", SceneDocument::default());

        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::MarkExternalChanged {
                document: Some(document),
            },
        );
        assert_eq!(
            workspace.scene(document).unwrap().external_reload(),
            EditorExternalReloadState::Pending
        );

        workspace
            .reload_external_session(
                Some(document),
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

        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("dirty"),
            },
        );
        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::MarkExternalChanged {
                document: Some(document),
            },
        );
        assert_eq!(
            workspace.scene(document).unwrap().external_reload(),
            EditorExternalReloadState::Conflict
        );

        let replacement = SceneAuthoringSession::new(scene_with_entity(entity_id("rejected")));
        let replacement_revision = replacement.revision();
        let error = workspace
            .reload_external_session(Some(document), replacement)
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

        let report = workspace
            .reload_external_session(Some(document), replacement)
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
            .reload_external_session(Some(missing), replacement)
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
        assert!(session.sync_world(&mut world, &registry).synced);
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
    fn tooling_plugin_initializes_editor_workspace() {
        let mut app = App::new();
        app.add_plugin(ToolingPlugin).unwrap();

        assert!(app.world().contains_resource::<EditorWorkspace>());
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

    fn frozen_empty_registry() -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
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

    fn scene_with_entity(entity: SceneEntityId) -> SceneDocument {
        SceneDocument::new([SceneEntityRecord::new(entity)])
    }

    fn add_entity_patch(id: &str) -> ScenePatchDocument {
        ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(entity_id(id)),
        }])
    }

    fn entity_id(id: &str) -> SceneEntityId {
        SceneEntityId::new(id).unwrap()
    }
}
