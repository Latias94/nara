use std::collections::{BTreeMap, BTreeSet};

use nara_diagnostic::DiagnosticReport;
use nara_ecs::Resource;
use nara_reflect::ComponentRegistry;
use nara_scene::{
    SceneAuthoringRevision, SceneAuthoringSession, SceneDocument, SceneEntityId,
    ScenePatchDocument, ScenePatchReport,
};

use crate::diagnostic;
use crate::inspector::{SceneInspectorCommand, SceneInspectorCommandReport};
use crate::play::{
    SceneApplyChangesReport, SceneEditorModel, SceneEditorState, ScenePlayTransitionReport,
};
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
    next_document_id: u64,
    active_document: Option<EditorDocumentId>,
    scenes: BTreeMap<EditorDocumentId, EditorSceneSlot>,
}

impl EditorWorkspace {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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

    pub fn scene_mut(&mut self, document: EditorDocumentId) -> Option<&mut EditorSceneSlot> {
        self.scenes.get_mut(&document)
    }

    pub fn active_scene_mut(&mut self) -> Option<&mut EditorSceneSlot> {
        let document = self.active_document?;
        self.scenes.get_mut(&document)
    }

    pub fn open_scene(
        &mut self,
        title: impl Into<String>,
        document: SceneDocument,
    ) -> EditorWorkspaceCommandReport {
        let document_id = self.allocate_document_id();
        let slot = EditorSceneSlot::new(title, document);
        self.scenes.insert(document_id, slot);
        self.active_document = Some(document_id);
        EditorWorkspaceCommandReport {
            applied: true,
            document: Some(document_id),
            opened_document: Some(document_id),
            active_document: self.active_document,
            dirty: Some(false),
            revision: self.scenes.get(&document_id).map(|slot| slot.revision()),
            diagnostics: DiagnosticReport::default(),
            ..EditorWorkspaceCommandReport::default()
        }
    }

    pub fn apply_command(
        &mut self,
        registry: &ComponentRegistry,
        command: EditorWorkspaceCommand,
    ) -> EditorWorkspaceCommandReport {
        match command {
            EditorWorkspaceCommand::OpenScene { title, document } => {
                self.open_scene(title, document)
            }
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
            EditorWorkspaceCommand::MarkSaved { document } => self.mark_saved(document),
            EditorWorkspaceCommand::MarkExternalChanged { document } => {
                self.mark_external_changed(document)
            }
            EditorWorkspaceCommand::ReloadExternalDocument { document, scene } => {
                self.reload_external_document(document, scene)
            }
            EditorWorkspaceCommand::StartPlay { document } => self.start_play(registry, document),
            EditorWorkspaceCommand::PausePlay { document } => self.pause_play(document),
            EditorWorkspaceCommand::ResumePlay { document } => self.resume_play(document),
            EditorWorkspaceCommand::StopPlay { document } => self.stop_play(document),
            EditorWorkspaceCommand::ApplyChangesStatus { document } => {
                self.apply_changes_status(document)
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
        if self.scenes.remove(&document).is_none() {
            return workspace_document_error_report(context, document);
        }
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

    fn mark_saved(&mut self, document: Option<EditorDocumentId>) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        slot.mark_saved();
        report_for_slot(document, active_document, slot)
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

    fn reload_external_document(
        &mut self,
        document: Option<EditorDocumentId>,
        scene: SceneDocument,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        if slot.is_dirty() {
            slot.external_reload = EditorExternalReloadState::Conflict;
            return workspace_error_report(
                context,
                "tooling.workspace-reload-conflict",
                "external scene reload cannot replace a dirty editor document",
            );
        }
        slot.session.replace_document(scene);
        slot.selection.clear();
        slot.editor = SceneEditorState::new();
        slot.mark_saved();
        slot.external_reload = EditorExternalReloadState::Clean;
        report_for_slot(document, active_document, slot)
    }

    fn start_play(
        &mut self,
        registry: &ComponentRegistry,
        document: Option<EditorDocumentId>,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        let play_report = slot.editor.start_play(&slot.session, registry);
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = play_report.applied;
        report.play_report = Some(play_report.clone());
        let _ = report.diagnostics.extend(play_report.diagnostics);
        report
    }

    fn pause_play(&mut self, document: Option<EditorDocumentId>) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        let play_report = slot.editor.pause_play();
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = play_report.applied;
        report.play_report = Some(play_report.clone());
        let _ = report.diagnostics.extend(play_report.diagnostics);
        report
    }

    fn resume_play(&mut self, document: Option<EditorDocumentId>) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        let play_report = slot.editor.resume_play();
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = play_report.applied;
        report.play_report = Some(play_report.clone());
        let _ = report.diagnostics.extend(play_report.diagnostics);
        report
    }

    fn stop_play(&mut self, document: Option<EditorDocumentId>) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        let play_report = slot.editor.stop_play();
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = play_report.applied;
        report.play_report = Some(play_report.clone());
        let _ = report.diagnostics.extend(play_report.diagnostics);
        report
    }

    fn apply_changes_status(
        &mut self,
        document: Option<EditorDocumentId>,
    ) -> EditorWorkspaceCommandReport {
        let active_document = self.active_document;
        let context = WorkspaceDocumentContext::new(active_document, document);
        let Some((document, slot)) = self.resolve_scene_mut(document) else {
            return workspace_resolution_error_report(context);
        };
        let apply_changes_report = slot.editor.apply_changes_status(&slot.session);
        let mut report = report_for_slot(document, active_document, slot);
        report.applied = apply_changes_report.applied;
        report.apply_changes_report = Some(apply_changes_report.clone());
        let _ = report.diagnostics.extend(apply_changes_report.diagnostics);
        report
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
    external_reload: EditorExternalReloadState,
}

impl EditorSceneSlot {
    #[must_use]
    pub fn new(title: impl Into<String>, document: SceneDocument) -> Self {
        let session = SceneAuthoringSession::new(document);
        let saved_revision = session.revision();
        Self {
            title: title.into(),
            session,
            editor: SceneEditorState::new(),
            selection: EditorSelectionSet::default(),
            saved_revision,
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

    pub fn session_mut(&mut self) -> &mut SceneAuthoringSession {
        &mut self.session
    }

    #[must_use]
    pub fn editor(&self) -> &SceneEditorState {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut SceneEditorState {
        &mut self.editor
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
    pub fn is_dirty(&self) -> bool {
        self.session.revision() != self.saved_revision
    }

    #[must_use]
    pub fn external_reload(&self) -> EditorExternalReloadState {
        self.external_reload
    }

    pub fn mark_saved(&mut self) {
        self.saved_revision = self.session.revision();
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
    OpenScene {
        title: String,
        document: SceneDocument,
    },
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
    MarkSaved {
        document: Option<EditorDocumentId>,
    },
    MarkExternalChanged {
        document: Option<EditorDocumentId>,
    },
    ReloadExternalDocument {
        document: Option<EditorDocumentId>,
        scene: SceneDocument,
    },
    StartPlay {
        document: Option<EditorDocumentId>,
    },
    PausePlay {
        document: Option<EditorDocumentId>,
    },
    ResumePlay {
        document: Option<EditorDocumentId>,
    },
    StopPlay {
        document: Option<EditorDocumentId>,
    },
    ApplyChangesStatus {
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
    pub play_report: Option<ScenePlayTransitionReport>,
    pub apply_changes_report: Option<SceneApplyChangesReport>,
    pub diagnostics: DiagnosticReport,
}

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
    fn two_open_scene_slots_keep_selection_and_dirty_isolated() {
        let mut workspace = EditorWorkspace::new();
        let registry = ComponentRegistry::default();
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
        let registry = ComponentRegistry::default();
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
        let registry = ComponentRegistry::default();
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
        let registry = ComponentRegistry::default();
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
        let registry = ComponentRegistry::default();
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
        let registry = ComponentRegistry::default();
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

        workspace.apply_command(
            &registry,
            EditorWorkspaceCommand::ReloadExternalDocument {
                document: Some(document),
                scene: scene_with_entity(entity_id("fresh")),
            },
        );
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

    fn open_scene(
        workspace: &mut EditorWorkspace,
        title: &str,
        document: SceneDocument,
    ) -> EditorDocumentId {
        workspace
            .open_scene(title.to_owned(), document)
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
