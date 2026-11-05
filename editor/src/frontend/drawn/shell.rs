use std::{collections::HashMap, sync::Arc};

use nara::tooling::{
    EditorAction, EditorAuthoringReadiness, EditorCloseDecision, EditorDocumentId,
    EditorDocumentKind, EditorDocumentMutation, EditorDocumentOpenPhase, EditorDocumentOpenView,
    EditorDocumentTabModel, EditorFrameModel, EditorFrameRevision, EditorNavigationAction,
    EditorPersistenceAction, EditorPersistenceRequestKind, EditorPersistenceView, EditorPlayState,
    EditorProjectBrowserAction, EditorProjectBrowserEntry, EditorProjectBrowserStatus,
    EditorProjectBrowserView, EditorProjectReadinessView, EditorProjectSourceOpenCapability,
    EditorReopenTarget, EditorRuntimeAvailability, EditorWorkspaceIntent,
    EditorWorkspaceIntentAction, EditorWorkspaceIntentPhase, EditorWorkspaceIntentView,
};
use nara_text::FontFace;
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiCompiler, UiCustomLayout, UiDimension,
    UiDispatchBatch, UiEdges, UiEnvironment, UiFrame, UiKey, UiLayer, UiLocalKey, UiNode,
    UiOverflow, UiPositionMode, UiRect, UiRoot, UiRootKey, UiRootStatus, UiStyle, UiTextOverflow,
    UiTheme, UiVisual,
};
use nara_ui_widgets::{
    UiAnchoredPopup, UiAppearance, UiButton, UiButtonState, UiButtonVariant, UiControlGrid,
    UiControlSize, UiIntentBatch, UiTextRole,
};

use crate::{PanelActionDispatch, PanelSubmissionId};

use super::super::controls;

mod feedback;
#[cfg(test)]
mod tests;

const DOCUMENTS_PER_PAGE: usize = 4;
const SOURCES_PER_PAGE: usize = 4;
const RETAINED_FRAMES: usize = 3;
const MENU_CONTROLS: [Control; 4] = [
    Control::MenuRefresh,
    Control::MenuReopen,
    Control::MenuClose,
    Control::DismissMenu,
];

#[derive(Clone, PartialEq, Eq)]
struct ShellBasis {
    documents: Vec<EditorDocumentTabModel>,
    active: Option<EditorDocumentId>,
    readiness: EditorProjectReadinessView,
    play: EditorPlayState,
    persistence: EditorPersistenceView,
    reopen: EditorReopenTarget,
    workspace_intent: EditorWorkspaceIntentView,
    browser: EditorProjectBrowserView,
    document_open: EditorDocumentOpenView,
    editing: bool,
}

impl ShellBasis {
    fn capture(model: &EditorFrameModel) -> Self {
        Self {
            documents: model.workspace().documents.clone(),
            active: model.workspace().active_document,
            readiness: model.project_readiness(),
            play: model.play().state(),
            persistence: model.persistence(),
            reopen: model.reopen_target(),
            workspace_intent: model.workspace_intent(),
            browser: model.project_browser().clone(),
            document_open: model.document_open().clone(),
            editing: controls::editing_enabled(model),
        }
    }

    fn document(&self, id: EditorDocumentId) -> Option<&EditorDocumentTabModel> {
        self.documents.iter().find(|tab| tab.document == id)
    }

    fn active_document(&self) -> Option<&EditorDocumentTabModel> {
        self.document(self.active?)
    }

    fn persistence_idle(&self) -> bool {
        self.workspace_intent.intent().is_none()
            && self.persistence.operation().is_none()
            && self.active_document().is_some_and(|document| {
                document.kind != EditorDocumentKind::Scene
                    || self.play == EditorPlayState::Empty
                    || self.readiness.runtime() == EditorRuntimeAvailability::ExternalSupervised
            })
    }

    fn sources(&self) -> impl Iterator<Item = (usize, &EditorProjectBrowserEntry)> {
        self.browser
            .snapshot()
            .into_iter()
            .flat_map(|snapshot| snapshot.entries())
            .enumerate()
            .filter(|(_, entry)| entry.open_capability().is_document())
    }

    fn source(&self, index: usize) -> Option<SourceBinding> {
        let snapshot = self.browser.snapshot()?;
        let entry = snapshot.entries().get(index)?;
        entry
            .open_capability()
            .is_document()
            .then(|| SourceBinding {
                snapshot_generation: snapshot.generation(),
                snapshot_index: index,
                relative_path: Arc::from(entry.relative_path()),
                open_capability: entry.open_capability(),
            })
    }

    fn action(&self, control: Control) -> Option<EditorAction> {
        match control {
            Control::RefreshSources => (self.workspace_intent.intent().is_none()
                && self.persistence.operation().is_none()
                && !self.document_open.has_pending_work()
                && self.document_open.result().is_none()
                && !self.browser.has_pending_work())
            .then_some(EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Refresh,
            )),
            Control::OpenSource(index) => {
                let source = self.source(index)?;
                (matches!(
                    self.browser.status(),
                    EditorProjectBrowserStatus::Ready
                        | EditorProjectBrowserStatus::ReadyWithDiagnostics
                ) && self.workspace_intent.intent().is_none()
                    && self.persistence.operation().is_none()
                    && !self.document_open.has_pending_work()
                    && self.document_open.result().is_none())
                .then_some(EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Open {
                        relative_path: source.relative_path,
                    },
                ))
            }
            Control::CancelOpen => {
                let (request, _, phase) = self.document_open.pending()?;
                (phase == EditorDocumentOpenPhase::Preparing).then_some(
                    EditorAction::ProjectBrowser(EditorProjectBrowserAction::CancelOpen {
                        request,
                    }),
                )
            }
            Control::AcknowledgeOpen => self.document_open.result().map(|result| {
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                    request: result.request(),
                })
            }),
            Control::Document(id) => {
                let tab = self.document(id)?;
                (self.workspace_intent.intent().is_none()
                    && (!tab.active || tab.kind == EditorDocumentKind::Scene))
                    .then_some(EditorAction::Navigate(
                        EditorNavigationAction::SetActiveDocument { document: id },
                    ))
            }
            Control::Undo | Control::Redo => {
                let document = self.active_document()?;
                let available = if control == Control::Undo {
                    document.history.undo_depth
                } else {
                    document.history.redo_depth
                };
                if !self.editing || available == 0 {
                    return None;
                }
                let mutation = if control == Control::Undo {
                    EditorDocumentMutation::Undo {
                        document: document.document,
                        base_revision: document.revision,
                    }
                } else {
                    EditorDocumentMutation::Redo {
                        document: document.document,
                        base_revision: document.revision,
                    }
                };
                Some(EditorAction::Mutate(mutation))
            }
            Control::Save => {
                let document = self.active_document()?;
                (self.persistence_idle()
                    && document.dirty
                    && controls::persistence_allows(
                        self.persistence.result(),
                        EditorPersistenceRequestKind::Save,
                        document.document,
                    ))
                .then_some(EditorAction::Persistence(
                    EditorPersistenceAction::Save {
                        document: document.document,
                        base_revision: document.revision,
                    },
                ))
            }
            Control::Reopen => {
                let EditorReopenTarget::OpenDocument {
                    document,
                    base_revision,
                } = self.reopen
                else {
                    return None;
                };
                (self.persistence_idle()
                    && self.document(document)?.revision == base_revision
                    && controls::persistence_allows(
                        self.persistence.result(),
                        EditorPersistenceRequestKind::Reopen,
                        document,
                    ))
                .then_some(EditorAction::Persistence(
                    EditorPersistenceAction::Reopen {
                        target: self.reopen,
                    },
                ))
            }
            Control::Close => {
                let document = self.active_document()?.document;
                (self.workspace_intent.intent().is_none()
                    && self.workspace_intent.result().is_none())
                .then_some(EditorAction::WorkspaceIntent(
                    EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                        document,
                    }),
                ))
            }
            Control::AcknowledgePersistence => match self.persistence {
                EditorPersistenceView::Terminal { attempt, .. } => Some(EditorAction::Persistence(
                    EditorPersistenceAction::AcknowledgeResult { attempt },
                )),
                _ => None,
            },
            Control::SaveBeforeClose | Control::DiscardBeforeClose | Control::CancelClose => {
                if self.workspace_intent.phase()
                    != Some(EditorWorkspaceIntentPhase::AwaitingDecision)
                {
                    return None;
                }
                let prompt = self.workspace_intent.decision_prompt()?;
                if self.workspace_intent.decision_document() != Some(prompt.document()) {
                    return None;
                }
                self.document(prompt.document())?;
                let decision = match control {
                    Control::SaveBeforeClose => {
                        if self.persistence.operation().is_some()
                            || !controls::persistence_allows(
                                self.persistence.result(),
                                EditorPersistenceRequestKind::Save,
                                prompt.document(),
                            )
                        {
                            return None;
                        }
                        EditorCloseDecision::Save
                    }
                    Control::DiscardBeforeClose => EditorCloseDecision::Discard,
                    _ => EditorCloseDecision::Cancel,
                };
                Some(EditorAction::WorkspaceIntent(prompt.resolve(decision)))
            }
            Control::AcknowledgeClose => self.workspace_intent.result().map(|_| {
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult)
            }),
            Control::MenuRefresh => self.action(Control::RefreshSources),
            Control::MenuReopen => self.action(Control::Reopen),
            Control::MenuClose => self.action(Control::Close),
            Control::OpenMenu | Control::DismissMenu => None,
            Control::PreviousDocuments
            | Control::NextDocuments
            | Control::PreviousSources
            | Control::NextSources => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    OpenMenu,
    DismissMenu,
    MenuRefresh,
    MenuReopen,
    MenuClose,
    RefreshSources,
    OpenSource(usize),
    PreviousSources,
    NextSources,
    CancelOpen,
    AcknowledgeOpen,
    Document(EditorDocumentId),
    PreviousDocuments,
    NextDocuments,
    Undo,
    Redo,
    Save,
    Reopen,
    Close,
    AcknowledgePersistence,
    SaveBeforeClose,
    DiscardBeforeClose,
    CancelClose,
    AcknowledgeClose,
}

impl Control {
    fn identity(self) -> String {
        match self {
            Self::OpenMenu => "menu.open".into(),
            Self::DismissMenu => "menu.dismiss".into(),
            Self::MenuRefresh => "menu.refresh".into(),
            Self::MenuReopen => "menu.reopen".into(),
            Self::MenuClose => "menu.close".into(),
            Self::RefreshSources => "sources.refresh".into(),
            Self::OpenSource(index) => format!("source.{index}"),
            Self::PreviousSources => "sources.previous".into(),
            Self::NextSources => "sources.next".into(),
            Self::CancelOpen => "source.cancel".into(),
            Self::AcknowledgeOpen => "source.acknowledge".into(),
            Self::Document(document) => format!("document.{}", document.raw()),
            Self::PreviousDocuments => "documents.previous".into(),
            Self::NextDocuments => "documents.next".into(),
            Self::Undo => "undo".into(),
            Self::Redo => "redo".into(),
            Self::Save => "save".into(),
            Self::Reopen => "reopen".into(),
            Self::Close => "close".into(),
            Self::AcknowledgePersistence => "persistence.acknowledge".into(),
            Self::SaveBeforeClose => "close.save".into(),
            Self::DiscardBeforeClose => "close.discard".into(),
            Self::CancelClose => "close.cancel".into(),
            Self::AcknowledgeClose => "close.acknowledge".into(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct SourceBinding {
    snapshot_generation: u64,
    snapshot_index: usize,
    relative_path: Arc<str>,
    open_capability: EditorProjectSourceOpenCapability,
}

#[derive(Clone)]
struct Binding {
    control: Control,
    button: UiButton,
    source: Option<SourceBinding>,
}

struct CompiledShell {
    model_revision: EditorFrameRevision,
    basis: ShellBasis,
    frame: Arc<UiFrame>,
    generation: u64,
    offset: usize,
    source_offset: usize,
    appearance: UiAppearance,
    scroll: f32,
    buttons: HashMap<UiKey, Binding>,
}

pub(in crate::frontend::drawn) struct ShellControls {
    root: UiRootKey,
    menu_root: UiRootKey,
    compiler: UiCompiler,
    frames: Vec<CompiledShell>,
    basis: Option<ShellBasis>,
    states: HashMap<UiKey, UiButtonState>,
    generation: u64,
    next_snapshot: u64,
    document_offset: usize,
    source_offset: usize,
    appearance: UiAppearance,
    scroll: f32,
    scroll_limit: f32,
    menu_anchor: Option<UiRect>,
    menu_environment: Option<UiEnvironment>,
    menu_scroll: f32,
    menu_scroll_limit: f32,
    menu_navigation: Option<Control>,
    focus_request: Option<Control>,
    paint_dirty: bool,
    last_result: Option<PanelSubmissionId>,
    result_count: u64,
    failed_count: u64,
    feedback: String,
}

impl ShellControls {
    pub(in crate::frontend::drawn) fn new(
        root: UiRootKey,
        menu_root: UiRootKey,
        fonts: Vec<FontFace>,
    ) -> Result<Self, String> {
        if root == menu_root {
            return Err("Workspace and action-menu roots must be distinct".into());
        }
        Ok(Self {
            root,
            menu_root,
            compiler: UiCompiler::with_fonts(fonts, Default::default())
                .map_err(|error| error.to_string())?,
            frames: Vec::new(),
            basis: None,
            states: HashMap::new(),
            generation: 1,
            next_snapshot: 1,
            document_offset: 0,
            source_offset: 0,
            appearance: UiAppearance::default(),
            scroll: 0.0,
            scroll_limit: 0.0,
            menu_anchor: None,
            menu_environment: None,
            menu_scroll: 0.0,
            menu_scroll_limit: 0.0,
            menu_navigation: None,
            focus_request: None,
            paint_dirty: true,
            last_result: None,
            result_count: 0,
            failed_count: 0,
            feedback: "No actions submitted".into(),
        })
    }

    pub(in crate::frontend::drawn) fn retains_frame(&self, frame: &Arc<UiFrame>) -> bool {
        self.frames
            .iter()
            .any(|compiled| Arc::ptr_eq(&compiled.frame, frame))
    }

    pub(in crate::frontend::drawn) fn input_basis_is_current(&self, frame: &Arc<UiFrame>) -> bool {
        self.retained(frame)
            .is_ok_and(|compiled| compiled.generation == self.generation)
    }

    fn retained(&self, frame: &Arc<UiFrame>) -> Result<&CompiledShell, String> {
        self.frames
            .iter()
            .find(|compiled| Arc::ptr_eq(&compiled.frame, frame))
            .ok_or_else(|| "The workspace shell frame has retired".into())
    }

    pub(in crate::frontend::drawn) fn owns_action(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        action: &UiActionId,
    ) -> bool {
        self.retained(frame).is_ok_and(|compiled| {
            compiled.generation == self.generation
                && compiled.offset == self.document_offset
                && compiled.source_offset == self.source_offset
                && self.basis.as_ref() == Some(&compiled.basis)
                && compiled.buttons.get(key).is_some_and(|binding| {
                    binding.button.action() == action
                        && match binding.control {
                            Control::OpenSource(index) => {
                                binding.source == compiled.basis.source(index)
                            }
                            _ => true,
                        }
                })
        })
    }

    pub(in crate::frontend::drawn) fn cancel_input(&mut self) -> Result<(), String> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "Workspace shell input identities are exhausted".to_owned())?;
        self.states.clear();
        self.menu_anchor = None;
        self.menu_environment = None;
        self.menu_scroll = 0.0;
        self.menu_scroll_limit = 0.0;
        self.menu_navigation = None;
        self.focus_request = None;
        self.paint_dirty = true;
        Ok(())
    }

    pub(in crate::frontend::drawn) fn set_appearance(
        &mut self,
        appearance: UiAppearance,
    ) -> Result<(), String> {
        if self.appearance != appearance {
            self.cancel_input()?;
            self.appearance = appearance;
        }
        Ok(())
    }

    pub(in crate::frontend::drawn) fn dispatch(
        &mut self,
        frame: &Arc<UiFrame>,
        batch: &UiDispatchBatch,
    ) -> Result<UiIntentBatch, String> {
        let compiled = self.retained(frame)?;
        if batch.frame_revision() != frame.revision() {
            return Err("The input batch belongs to another workspace shell frame".into());
        }
        let mut intents = UiIntentBatch::default();
        if compiled.generation != self.generation
            || compiled.offset != self.document_offset
            || compiled.source_offset != self.source_offset
        {
            return Ok(intents);
        }
        let mut states = self.states.clone();
        for binding in compiled.buttons.values() {
            states
                .entry(binding.button.key().clone())
                .or_default()
                .reduce_batch(&binding.button, batch, &mut intents)
                .map_err(|error| error.to_string())?;
        }
        if self.states != states {
            self.states = states;
            self.paint_dirty = true;
        }
        Ok(intents)
    }

    pub(in crate::frontend::drawn) fn activate(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        model: &EditorFrameModel,
    ) -> Result<Option<EditorAction>, String> {
        let result = self.activate_inner(frame, key, model);
        if let Err(error) = &result {
            self.feedback = bounded_text(error, 96);
            self.paint_dirty = true;
        }
        result
    }

    fn activate_inner(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        model: &EditorFrameModel,
    ) -> Result<Option<EditorAction>, String> {
        let compiled = self.retained(frame)?;
        if compiled.generation != self.generation
            || compiled.offset != self.document_offset
            || compiled.source_offset != self.source_offset
            || self.basis.as_ref() != Some(&compiled.basis)
            || ShellBasis::capture(model) != compiled.basis
        {
            return Err(
                "The source, document or workspace decision changed; use the current controls"
                    .into(),
            );
        }
        let binding = compiled.buttons.get(key).ok_or_else(|| {
            "The control does not belong to this workspace shell frame".to_owned()
        })?;
        if !binding.button.enabled() {
            return Err("This workspace control is unavailable".into());
        }
        let control = binding.control;
        if let Control::OpenSource(index) = control
            && binding.source != compiled.basis.source(index)
        {
            return Err("The original source binding changed; use the current controls".into());
        }
        match control {
            Control::OpenMenu => {
                let anchor = frame
                    .semantics()
                    .iter()
                    .find(|node| node.key() == key)
                    .map(|node| node.rect())
                    .ok_or("The action menu anchor is no longer presented")?;
                let first = MENU_CONTROLS.into_iter().find(|control| {
                    *control == Control::DismissMenu || compiled.basis.action(*control).is_some()
                });
                self.cancel_input()?;
                self.menu_anchor = Some(anchor);
                self.menu_environment = Some(frame.environment());
                self.focus_request = first;
                self.menu_navigation = first;
                Ok(None)
            }
            Control::DismissMenu => {
                self.dismiss_menu()?;
                Ok(None)
            }
            Control::PreviousDocuments | Control::NextDocuments => {
                let next =
                    if control == Control::PreviousDocuments {
                        self.document_offset.saturating_sub(DOCUMENTS_PER_PAGE)
                    } else {
                        self.document_offset.saturating_add(DOCUMENTS_PER_PAGE).min(
                            last_page_offset(compiled.basis.documents.len(), DOCUMENTS_PER_PAGE),
                        )
                    };
                self.cancel_input()?;
                self.document_offset = next;
                Ok(None)
            }
            Control::PreviousSources | Control::NextSources => {
                let next = if control == Control::PreviousSources {
                    self.source_offset.saturating_sub(SOURCES_PER_PAGE)
                } else {
                    self.source_offset
                        .saturating_add(SOURCES_PER_PAGE)
                        .min(last_page_offset(
                            compiled.basis.sources().count(),
                            SOURCES_PER_PAGE,
                        ))
                };
                self.cancel_input()?;
                self.source_offset = next;
                Ok(None)
            }
            _ => {
                let action = compiled.basis.action(control).ok_or_else(|| {
                    "The original document action is no longer available".to_owned()
                })?;
                self.cancel_input()?;
                Ok(Some(action))
            }
        }
    }

    pub(in crate::frontend::drawn) fn action_finished(
        &mut self,
        id: PanelSubmissionId,
        result: &PanelActionDispatch,
    ) {
        if self.last_result.is_some_and(|previous| id <= previous) {
            return;
        }
        self.last_result = Some(id);
        self.result_count = self.result_count.saturating_add(1);
        let (applied, message) = feedback::action_result(result);
        self.failed_count = self.failed_count.saturating_add(u64::from(!applied));
        self.feedback = format!(
            "Result {}: {}",
            self.result_count,
            bounded_text(&message, 62)
        );
        self.paint_dirty = true;
    }

    pub(in crate::frontend::drawn) fn compile(
        &mut self,
        model: &EditorFrameModel,
        environment: UiEnvironment,
    ) -> Result<Arc<UiFrame>, String> {
        let inset = self.appearance.panel_inset();
        let width = environment.logical_size().width() - 2.0 * inset;
        let minimum_height = 96.0_f32.max(
            self.appearance
                .control_metrics(UiControlSize::Small)
                .map_err(|error| error.to_string())?
                .height()
                + 32.0,
        );
        if width < 296.0 || environment.logical_size().height() < minimum_height {
            return Err(format!(
                "The workspace shell needs at least 320 by {minimum_height} logical pixels for the current appearance"
            ));
        }
        let basis = ShellBasis::capture(model);
        if self.menu_anchor.is_some() && self.menu_environment != Some(environment) {
            self.dismiss_menu()?;
        }
        let offset = self
            .document_offset
            .min(last_page_offset(basis.documents.len(), DOCUMENTS_PER_PAGE));
        let source_count = basis.sources().count();
        let source_offset = self
            .source_offset
            .min(last_page_offset(source_count, SOURCES_PER_PAGE));
        if self.basis.as_ref() != Some(&basis)
            || self.document_offset != offset
            || self.source_offset != source_offset
        {
            self.cancel_input()?;
            self.document_offset = offset;
            self.source_offset = source_offset;
            self.basis = Some(basis.clone());
        }
        if !self.paint_dirty
            && let Some(previous) = self.frames.last()
            && previous.model_revision == model.revision()
            && previous.basis == basis
            && previous.generation == self.generation
            && previous.offset == self.document_offset
            && previous.source_offset == self.source_offset
            && previous.appearance == self.appearance
            && previous.scroll == self.scroll
            && previous.frame.environment() == environment
        {
            return Ok(Arc::clone(&previous.frame));
        }

        let mut groups = Vec::new();
        let mut buttons = HashMap::new();
        let count = basis.documents.len();
        let first = if count == 0 {
            0
        } else {
            self.document_offset + 1
        };
        let last = self
            .document_offset
            .saturating_add(DOCUMENTS_PER_PAGE)
            .min(count);
        let dirty = basis
            .documents
            .iter()
            .filter(|document| document.dirty)
            .count();
        let readiness = match basis.readiness.authoring() {
            EditorAuthoringReadiness::Complete => "ready",
            EditorAuthoringReadiness::Degraded => "needs attention",
        };
        let project = self.label(
            "project",
            &format!(
                "Nara | Project: {readiness} | Documents {first}-{last}/{count} | {dirty} unsaved",
            ),
        )?;
        let result = self.label(
            "action-result",
            &format!("{} | {} refused", self.feedback, self.failed_count),
        )?;
        groups.push(self.group(240.0, vec![project, result])?);
        let (document_text, source_text) = if let Some(document) = basis.active_document() {
            (
                format!(
                    "{} #{}: {} | {} | Undo {} / Redo {}",
                    kind_label(document.kind),
                    document.document.raw(),
                    bounded_text(&document.title, 12),
                    if document.dirty { "Unsaved" } else { "Saved" },
                    document.history.undo_depth,
                    document.history.redo_depth,
                ),
                format!(
                    "Source: {}",
                    document.source_binding.as_ref().map_or_else(
                        || "No saved source binding".into(),
                        |binding| source_label(binding.locator()),
                    ),
                ),
            )
        } else {
            ("No active document".into(), "Source: none selected".into())
        };
        groups.push(self.group(
            260.0,
            vec![
                self.label("document", &document_text)?,
                self.label("source", &source_text)?,
            ],
        )?);

        let mut tabs = vec![self.button(
            &basis,
            Control::PreviousDocuments,
            "Previous documents",
            self.document_offset > 0,
            false,
            &mut buttons,
        )?];
        for document in basis
            .documents
            .iter()
            .skip(self.document_offset)
            .take(DOCUMENTS_PER_PAGE)
        {
            let control = Control::Document(document.document);
            tabs.push(self.button(
                &basis,
                control,
                &format!(
                    "{}{} #{} {}",
                    if document.active { "> " } else { "" },
                    bounded_text(&document.title, 12),
                    document.document.raw(),
                    if document.dirty { "*" } else { "" },
                ),
                basis.action(control).is_some(),
                document.active,
                &mut buttons,
            )?);
        }
        tabs.push(self.button(
            &basis,
            Control::NextDocuments,
            "Next documents",
            self.document_offset < last_page_offset(basis.documents.len(), DOCUMENTS_PER_PAGE),
            false,
            &mut buttons,
        )?);
        groups.push(self.group(110.0, tabs)?);

        let mut toolbar = Vec::new();
        for (control, text) in [
            (Control::Undo, "Undo"),
            (Control::Redo, "Redo"),
            (Control::Save, "Save"),
            (Control::Reopen, "Reopen"),
            (Control::Close, "Close document"),
            (Control::OpenMenu, "Document actions"),
        ]
        .into_iter()
        {
            toolbar.push(self.button(
                &basis,
                control,
                text,
                control == Control::OpenMenu || basis.action(control).is_some(),
                false,
                &mut buttons,
            )?);
        }
        groups.push(self.group(94.0, toolbar)?);
        groups.push(self.group(
            220.0,
            vec![
                self.label(
                    "persistence",
                    &feedback::persistence_status(basis.persistence),
                )?,
                self.label(
                    "close-status",
                    &feedback::workspace_status(&basis.workspace_intent, &basis.documents),
                )?,
            ],
        )?);

        let mut acknowledgments = vec![self.button(
            &basis,
            Control::AcknowledgePersistence,
            "Acknowledge persistence result",
            basis.action(Control::AcknowledgePersistence).is_some(),
            false,
            &mut buttons,
        )?];
        let decisions: &[(Control, &str)] = if basis.workspace_intent.phase()
            == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
        {
            &[
                (Control::SaveBeforeClose, "Save and continue"),
                (Control::DiscardBeforeClose, "Discard and continue"),
                (Control::CancelClose, "Cancel"),
            ]
        } else if basis.workspace_intent.result().is_some() {
            &[(Control::AcknowledgeClose, "Acknowledge close result")]
        } else {
            &[]
        };
        for &(control, text) in decisions {
            acknowledgments.push(self.button(
                &basis,
                control,
                text,
                basis.action(control).is_some(),
                false,
                &mut buttons,
            )?);
        }
        groups.push(self.group(180.0, acknowledgments)?);

        let source_first = if source_count == 0 {
            0
        } else {
            self.source_offset + 1
        };
        let source_last = self
            .source_offset
            .saturating_add(SOURCES_PER_PAGE)
            .min(source_count);
        let sources = self.label(
            "sources",
            &format!(
                "Sources {source_first}-{source_last}/{source_count} | {}",
                feedback::source_status(basis.browser.status()),
            ),
        )?;
        let open = self.label("open-status", &feedback::open_status(&basis.document_open))?;
        groups.push(self.group(240.0, vec![sources, open])?);
        let mut sources = Vec::new();
        for (index, entry) in basis
            .sources()
            .skip(self.source_offset)
            .take(SOURCES_PER_PAGE)
        {
            let control = Control::OpenSource(index);
            sources.push(self.button(
                &basis,
                control,
                &format!("Open {}", entry.relative_path()),
                basis.action(control).is_some(),
                false,
                &mut buttons,
            )?);
        }
        groups.push(self.group(180.0, sources)?);
        let mut source_actions = Vec::new();
        for (control, text, enabled) in [
            (
                Control::PreviousSources,
                "Previous sources",
                self.source_offset > 0,
            ),
            (
                Control::NextSources,
                "Next sources",
                self.source_offset < last_page_offset(source_count, SOURCES_PER_PAGE),
            ),
            (
                Control::CancelOpen,
                "Cancel source open",
                basis.action(Control::CancelOpen).is_some(),
            ),
            (
                Control::AcknowledgeOpen,
                "Acknowledge source open",
                basis.action(Control::AcknowledgeOpen).is_some(),
            ),
            (
                Control::RefreshSources,
                "Refresh sources",
                basis.action(Control::RefreshSources).is_some(),
            ),
        ]
        .into_iter()
        {
            source_actions.push(self.button(
                &basis,
                control,
                text,
                enabled,
                false,
                &mut buttons,
            )?);
        }
        groups.push(self.group(138.0, source_actions)?);

        let content_key = UiLocalKey::runtime("nara.shell.layout", "content")
            .map_err(|error| error.to_string())?;
        let content = UiNode::new(
            UiStyle::flex()
                .with_position_mode(UiPositionMode::Absolute)
                .with_left(UiDimension::Px(inset))
                .with_top(UiDimension::Px(inset - self.scroll))
                .with_width(UiDimension::Px(width))
                .with_gap(self.appearance.gap())
                .with_flex_shrink(0.0),
        )
        .with_key(content_key.clone())
        .with_children(groups);

        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill().with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::solid(self.appearance.palette().window))
        .with_child(content);
        let mut snapshot = UiAuthoringSnapshotBuilder::new(self.next_snapshot).with_root(root);
        if let Some(anchor) = self.menu_anchor {
            let mut menu = Vec::new();
            for (control, label) in [
                (Control::MenuRefresh, "Refresh project sources"),
                (Control::MenuReopen, "Reopen active document"),
                (Control::MenuClose, "Close active document"),
                (Control::DismissMenu, "Close actions menu"),
            ] {
                menu.push(self.button(
                    &basis,
                    control,
                    label,
                    control == Control::DismissMenu || basis.action(control).is_some(),
                    false,
                    &mut buttons,
                )?);
            }
            let content = UiNode::new(
                UiStyle::flex()
                    .with_gap(self.appearance.gap())
                    .with_flex_shrink(0.0)
                    .with_top(UiDimension::Px(-self.menu_scroll)),
            )
            .with_children(menu);
            let menu = UiNode::new(
                UiStyle::flex()
                    .with_padding(UiEdges::all(8.0).map_err(|error| error.to_string())?)
                    .with_overflow(UiOverflow::Clip),
            )
            .with_key(
                UiLocalKey::runtime("nara.shell.layout", "menu-viewport")
                    .map_err(|error| error.to_string())?,
            )
            .with_visual(UiVisual::solid(self.appearance.palette().raised))
            .with_child(content);
            snapshot = snapshot.with_root(
                UiRoot::new(self.menu_root.clone(), UiLayer::Modal, UiStyle::fill())
                    .with_custom_layout(UiCustomLayout::new(
                        UiAnchoredPopup::new(anchor, 272.0, 8.0, 4.0)
                            .map_err(|error| error.to_string())?,
                    ))
                    .with_child(menu),
            );
        }
        let snapshot = snapshot.build().map_err(|error| error.to_string())?;
        let next_snapshot = self
            .next_snapshot
            .checked_add(1)
            .ok_or_else(|| "Workspace shell snapshot identities are exhausted".to_owned())?;
        let theme = UiTheme::new(
            environment.theme_revision().get(),
            self.appearance.palette().kernel_palette(),
        )
        .map_err(|error| error.to_string())?;
        let frame = self
            .compiler
            .compile(&snapshot, environment, theme)
            .map_err(|error| error.to_string())?;
        self.next_snapshot = next_snapshot;
        let expected_roots = if self.menu_anchor.is_some() { 2 } else { 1 };
        if frame.root_outcomes().len() != expected_roots
            || frame
                .root_outcomes()
                .iter()
                .any(|outcome| outcome.status() != UiRootStatus::Published)
        {
            self.cancel_input()?;
            return Err("The workspace shell controls could not be published".into());
        }
        if self.menu_anchor.is_some() {
            let viewport = frame.layouts().iter().find(|layout| {
                layout.key().is_some_and(|key| {
                    key.local_key().is_some_and(|local| {
                        local
                            == &UiLocalKey::runtime("nara.shell.layout", "menu-viewport")
                                .expect("fixed menu identity is bounded")
                    })
                })
            });
            let available = viewport.map_or(0.0, |layout| {
                (layout.rect().size().height() - 16.0).max(0.0)
            });
            let content = MENU_CONTROLS.len() as f32
                * self
                    .appearance
                    .control_metrics(UiControlSize::Small)
                    .map_err(|error| error.to_string())?
                    .height()
                + 3.0 * self.appearance.gap();
            self.menu_scroll_limit = (content - available).max(0.0);
            if let Some(control) = self
                .focus_request
                .filter(|control| MENU_CONTROLS.contains(control))
                && let Some((key, _)) = buttons
                    .iter()
                    .find(|(_, binding)| binding.control == control && binding.button.enabled())
                && let Some(target) = frame
                    .layouts()
                    .iter()
                    .find(|layout| layout.key() == Some(key))
                && let Some(viewport) = viewport
            {
                let min = viewport.rect().origin().y() + 8.0;
                let max = viewport.rect().max().y() - 8.0;
                let delta = reveal_delta(target.rect(), min, max);
                let next = (self.menu_scroll + delta).clamp(0.0, self.menu_scroll_limit);
                if next != self.menu_scroll {
                    self.menu_scroll = next;
                    return self.compile(model, environment);
                }
            }
            if self.menu_scroll > self.menu_scroll_limit {
                self.menu_scroll = self.menu_scroll_limit;
                return self.compile(model, environment);
            }
        }
        self.scroll_limit = frame
            .layouts()
            .iter()
            .find(|layout| {
                layout
                    .key()
                    .is_some_and(|key| key.local_key() == Some(&content_key))
            })
            .map_or(0.0, |layout| {
                (layout.rect().size().height() + 2.0 * inset - environment.logical_size().height())
                    .max(0.0)
            });
        if self.scroll > self.scroll_limit {
            self.cancel_input()?;
            self.scroll = self.scroll_limit;
            return self.compile(model, environment);
        }
        if self.focus_request == Some(Control::OpenMenu)
            && self.menu_anchor.is_none()
            && let Some((key, _)) = buttons
                .iter()
                .find(|(_, binding)| binding.control == Control::OpenMenu)
            && let Some(target) = frame
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(key))
        {
            let delta = reveal_delta(
                target.rect(),
                inset,
                environment.logical_size().height() - inset,
            );
            let next = (self.scroll + delta).clamp(0.0, self.scroll_limit);
            if next != self.scroll {
                self.cancel_input()?;
                self.scroll = next;
                self.focus_request = Some(Control::OpenMenu);
                return self.compile(model, environment);
            }
        }
        self.states.retain(|key, _| buttons.contains_key(key));
        self.frames.push(CompiledShell {
            model_revision: model.revision(),
            basis,
            frame: Arc::clone(&frame),
            generation: self.generation,
            offset: self.document_offset,
            source_offset: self.source_offset,
            appearance: self.appearance,
            scroll: self.scroll,
            buttons,
        });
        if self.frames.len() > RETAINED_FRAMES {
            self.frames.remove(0);
        }
        self.paint_dirty = false;
        Ok(frame)
    }

    fn button(
        &self,
        basis: &ShellBasis,
        control: Control,
        text: &str,
        enabled: bool,
        selected: bool,
        buttons: &mut HashMap<UiKey, Binding>,
    ) -> Result<UiNode, String> {
        let source = match control {
            Control::OpenSource(index) => basis.source(index),
            _ => None,
        };
        let identity = source.as_ref().map_or_else(
            || control.identity(),
            |source| {
                format!(
                    "source.{}.{}",
                    source.snapshot_generation, source.snapshot_index
                )
            },
        );
        let root = if matches!(
            control,
            Control::MenuRefresh | Control::MenuReopen | Control::MenuClose | Control::DismissMenu
        ) {
            &self.menu_root
        } else {
            &self.root
        };
        let key = UiKey::scoped(
            root.clone(),
            UiLocalKey::runtime("nara.shell", format!("{}.{}", self.generation, identity))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let button = UiButton::new(
            key.clone(),
            if source.is_some() {
                bounded_source_text(text)
            } else {
                bounded_text(text, 80)
            },
            UiActionId::new("shell.activate").map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?
        .with_enabled(enabled);
        let state = self.states.get(&key).copied().unwrap_or_default();
        let variant = match control {
            Control::Save | Control::SaveBeforeClose => UiButtonVariant::Primary,
            Control::DiscardBeforeClose => UiButtonVariant::Destructive,
            Control::Document(_) | Control::OpenSource(_) => UiButtonVariant::Quiet,
            _ => UiButtonVariant::Secondary,
        };
        let node = self
            .appearance
            .button(&button, state, variant, UiControlSize::Small, selected)
            .and_then(|recipe| recipe.authored_node(&button, root, UiStyle::flex()))
            .map_err(|error| error.to_string())?;
        buttons.insert(
            key,
            Binding {
                control,
                button,
                source,
            },
        );
        Ok(node)
    }

    pub(in crate::frontend::drawn) fn dismiss_menu(&mut self) -> Result<bool, String> {
        if self.menu_anchor.is_none() {
            return Ok(false);
        }
        self.cancel_input()?;
        self.focus_request = Some(Control::OpenMenu);
        Ok(true)
    }

    pub(in crate::frontend::drawn) fn menu_outside(
        &self,
        frame: &Arc<UiFrame>,
        point: nara_ui_kernel::UiPoint,
    ) -> bool {
        self.menu_anchor.is_some()
            && self
                .retained(frame)
                .is_ok_and(|compiled| compiled.generation == self.generation)
            && frame
                .layouts()
                .iter()
                .find(|layout| {
                    layout.key().is_some_and(|key| {
                        key.root_key() == &self.menu_root
                            && key.local_key().is_some_and(|local| {
                                local
                                    == &UiLocalKey::runtime("nara.shell.layout", "menu-viewport")
                                        .expect("fixed menu identity is bounded")
                            })
                    })
                })
                .is_some_and(|layout| !layout.rect().contains(point))
    }

    pub(in crate::frontend::drawn) fn take_focus_request(
        &mut self,
        frame: &Arc<UiFrame>,
    ) -> Option<UiKey> {
        let control = self.focus_request?;
        let compiled = self.retained(frame).ok()?;
        let key = compiled
            .buttons
            .iter()
            .find(|(_, binding)| binding.control == control && binding.button.enabled())
            .map(|(key, _)| key.clone())?;
        let target = frame
            .semantics()
            .iter()
            .find(|node| node.key() == &key && node.enabled())?;
        if target
            .clip_rect()
            .is_some_and(|clip| target.rect().intersect(clip) != Some(target.rect()))
        {
            return None;
        }
        self.focus_request = None;
        Some(key)
    }

    pub(in crate::frontend::drawn) fn navigate_menu(
        &mut self,
        frame: &Arc<UiFrame>,
        previous: bool,
    ) -> Result<bool, String> {
        if self.menu_anchor.is_none() {
            return Ok(false);
        }
        let compiled = self.retained(frame)?;
        if compiled.generation != self.generation {
            return Ok(true);
        }
        let enabled = MENU_CONTROLS
            .into_iter()
            .filter(|control| {
                compiled
                    .buttons
                    .values()
                    .any(|binding| binding.control == *control && binding.button.enabled())
            })
            .collect::<Vec<_>>();
        let focused = compiled
            .buttons
            .iter()
            .find(|(key, binding)| {
                MENU_CONTROLS.contains(&binding.control)
                    && self.states.get(*key).is_some_and(|state| state.focused())
            })
            .map(|(_, binding)| binding.control)
            .or(self.menu_navigation);
        let current =
            focused.and_then(|control| enabled.iter().position(|candidate| *candidate == control));
        let next = match current {
            Some(index) if previous => (index + enabled.len() - 1) % enabled.len(),
            Some(index) => (index + 1) % enabled.len(),
            None if previous => enabled.len().saturating_sub(1),
            None => 0,
        };
        if let Some(control) = enabled.get(next).copied() {
            self.focus_request = Some(control);
            self.menu_navigation = Some(control);
            self.paint_dirty = true;
        }
        Ok(true)
    }

    fn group(&self, minimum_cell_width: f32, children: Vec<UiNode>) -> Result<UiNode, String> {
        Ok(UiNode::new(UiStyle::flex().with_flex_shrink(0.0))
            .with_custom_layout(UiCustomLayout::new(
                UiControlGrid::new(minimum_cell_width, self.appearance.gap())
                    .map_err(|error| error.to_string())?,
            ))
            .with_children(children))
    }

    fn label(&self, key: &str, text: &str) -> Result<UiNode, String> {
        let text = self
            .appearance
            .text(bounded_text(text, 180), UiTextRole::Caption, true)
            .map_err(|error| error.to_string())?
            .with_overflow(UiTextOverflow::Clip);
        Ok(UiNode::new(UiStyle::flex().with_overflow(UiOverflow::Clip))
            .with_key(
                UiLocalKey::runtime("nara.shell.label", key).map_err(|error| error.to_string())?,
            )
            .with_text(text))
    }

    pub(in crate::frontend::drawn) fn scroll(
        &mut self,
        frame: &Arc<UiFrame>,
        delta: f32,
    ) -> Result<bool, String> {
        if !delta.is_finite() {
            return Err("Workspace scroll delta must be finite".into());
        }
        if self.retained(frame)?.generation != self.generation {
            return Ok(false);
        }
        if self.menu_anchor.is_some() {
            self.focus_request = None;
            let next = (self.menu_scroll + delta).clamp(0.0, self.menu_scroll_limit);
            let changed = self.menu_scroll != next;
            self.menu_scroll = next;
            self.paint_dirty |= changed;
            return Ok(changed);
        }
        let next = (self.scroll + delta).clamp(0.0, self.scroll_limit);
        if next == self.scroll {
            return Ok(false);
        }
        self.cancel_input()?;
        self.scroll = next;
        Ok(true)
    }
}

fn reveal_delta(target: UiRect, min: f32, max: f32) -> f32 {
    if target.origin().y() < min {
        target.origin().y() - min
    } else {
        (target.max().y() - max).max(0.0)
    }
}

fn last_page_offset(count: usize, per_page: usize) -> usize {
    count.saturating_sub(1) / per_page * per_page
}

fn bounded_source_text(text: &str) -> String {
    const MAXIMUM_BYTES: usize = 128;
    let mut output = String::new();
    for character in text.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if output.len() + character.len_utf8() > MAXIMUM_BYTES {
            while output.len() + '…'.len_utf8() > MAXIMUM_BYTES {
                output.pop();
            }
            output.push('…');
            break;
        }
        output.push(character);
    }
    output
}

fn bounded_text(text: &str, maximum: usize) -> String {
    let mut characters = text.chars();
    let mut output = String::new();
    for character in characters.by_ref().take(maximum) {
        output.push(if character.is_control() {
            ' '
        } else {
            character
        });
    }
    if characters.next().is_some() {
        output.push('…');
    }
    output
}

fn source_label(source: &str) -> String {
    let maximum = 56;
    if source.chars().take(maximum + 1).count() <= maximum {
        return bounded_text(source, maximum);
    }
    let tail = source
        .chars()
        .rev()
        .take(maximum / 2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    format!("{}{tail}", bounded_text(source, maximum / 2))
}

fn kind_label(kind: EditorDocumentKind) -> String {
    match kind {
        EditorDocumentKind::Scene => "Scene".into(),
        EditorDocumentKind::Prefab => "Prefab".into(),
        EditorDocumentKind::Extension(name) => bounded_text(name, 20),
    }
}
