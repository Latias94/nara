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
    UiActionId, UiAuthoringSnapshotBuilder, UiBrush, UiColorRole, UiCompiler, UiDispatchBatch,
    UiEnvironment, UiFrame, UiKey, UiLayer, UiLocalKey, UiNode, UiOverflow, UiRoot, UiRootKey,
    UiRootStatus, UiStyle, UiText, UiTextOverflow, UiTheme, UiVisual,
};
use nara_ui_widgets::{UiButton, UiButtonState, UiButtonVisualState, UiIntentBatch};

use crate::{PanelActionDispatch, PanelSubmissionId};

use super::super::controls;

mod feedback;
#[cfg(test)]
mod tests;

const DOCUMENTS_PER_PAGE: usize = 4;
const SOURCES_PER_PAGE: usize = 4;
const RETAINED_FRAMES: usize = 3;
const INSET: f32 = 8.0;
const GAP: f32 = 6.0;
const TEXT_SIZE: f32 = 14.0;

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
            Control::PreviousDocuments
            | Control::NextDocuments
            | Control::PreviousSources
            | Control::NextSources => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
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
    buttons: HashMap<UiKey, Binding>,
}

pub(in crate::frontend::drawn) struct ShellControls {
    root: UiRootKey,
    compiler: UiCompiler,
    frames: Vec<CompiledShell>,
    basis: Option<ShellBasis>,
    states: HashMap<UiKey, UiButtonState>,
    generation: u64,
    next_snapshot: u64,
    document_offset: usize,
    source_offset: usize,
    paint_dirty: bool,
    last_result: Option<PanelSubmissionId>,
    result_count: u64,
    failed_count: u64,
    feedback: String,
}

impl ShellControls {
    pub(in crate::frontend::drawn) fn new(
        root: UiRootKey,
        fonts: Vec<FontFace>,
    ) -> Result<Self, String> {
        Ok(Self {
            root,
            compiler: UiCompiler::with_fonts(fonts, Default::default())
                .map_err(|error| error.to_string())?,
            frames: Vec::new(),
            basis: None,
            states: HashMap::new(),
            generation: 1,
            next_snapshot: 1,
            document_offset: 0,
            source_offset: 0,
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

    fn retained(&self, frame: &Arc<UiFrame>) -> Result<&CompiledShell, String> {
        self.frames
            .iter()
            .find(|compiled| Arc::ptr_eq(&compiled.frame, frame))
            .ok_or_else(|| "The workspace shell frame has retired".into())
    }

    pub(in crate::frontend::drawn) fn cancel_input(&mut self) -> Result<(), String> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "Workspace shell input identities are exhausted".to_owned())?;
        self.states.clear();
        self.paint_dirty = true;
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
        let width = environment.logical_size().width() - 2.0 * INSET;
        if width < 944.0 || environment.logical_size().height() < 272.0 {
            return Err("The workspace shell needs at least 960 by 272 logical pixels".into());
        }
        let basis = ShellBasis::capture(model);
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
            && previous.frame.environment() == environment
        {
            return Ok(Arc::clone(&previous.frame));
        }

        let mut nodes = Vec::new();
        let mut buttons = HashMap::new();
        let half = (width - GAP) / 2.0;
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
        nodes.push(label(
            "project",
            &format!(
                "Nara | Project: {readiness} | Documents {first}-{last}/{count} | {dirty} unsaved",
            ),
            [INSET, 6.0, half, 20.0],
        )?);
        nodes.push(label(
            "action-result",
            &format!("{} | {} refused", self.feedback, self.failed_count),
            [INSET + half + GAP, 6.0, half, 20.0],
        )?);
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
        nodes.push(label(
            "document",
            &document_text,
            [INSET, 28.0, half, 20.0],
        )?);
        nodes.push(label(
            "source",
            &source_text,
            [INSET + half + GAP, 28.0, half, 20.0],
        )?);

        let page_width = 148.0;
        let tab_width = (width - 2.0 * page_width - 5.0 * GAP) / DOCUMENTS_PER_PAGE as f32;
        nodes.push(self.button(
            &basis,
            Control::PreviousDocuments,
            "Previous documents",
            self.document_offset > 0,
            false,
            [INSET, 52.0, page_width, 32.0],
            &mut buttons,
        )?);
        for (index, document) in basis
            .documents
            .iter()
            .skip(self.document_offset)
            .take(DOCUMENTS_PER_PAGE)
            .enumerate()
        {
            let control = Control::Document(document.document);
            nodes.push(self.button(
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
                [
                    INSET + page_width + GAP + index as f32 * (tab_width + GAP),
                    52.0,
                    tab_width,
                    32.0,
                ],
                &mut buttons,
            )?);
        }
        nodes.push(self.button(
            &basis,
            Control::NextDocuments,
            "Next documents",
            self.document_offset < last_page_offset(basis.documents.len(), DOCUMENTS_PER_PAGE),
            false,
            [INSET + width - page_width, 52.0, page_width, 32.0],
            &mut buttons,
        )?);

        let toolbar_width = (width - 4.0 * GAP) / 5.0;
        for (index, (control, text)) in [
            (Control::Undo, "Undo"),
            (Control::Redo, "Redo"),
            (Control::Save, "Save"),
            (Control::Reopen, "Reopen"),
            (Control::Close, "Close document"),
        ]
        .into_iter()
        .enumerate()
        {
            nodes.push(self.button(
                &basis,
                control,
                text,
                basis.action(control).is_some(),
                false,
                [
                    INSET + index as f32 * (toolbar_width + GAP),
                    90.0,
                    toolbar_width,
                    32.0,
                ],
                &mut buttons,
            )?);
        }
        nodes.push(label(
            "persistence",
            &feedback::persistence_status(basis.persistence),
            [INSET, 126.0, half, 20.0],
        )?);
        nodes.push(label(
            "close-status",
            &feedback::workspace_status(&basis.workspace_intent, &basis.documents),
            [INSET + half + GAP, 126.0, half, 20.0],
        )?);

        let acknowledge_width = 268.0;
        nodes.push(self.button(
            &basis,
            Control::AcknowledgePersistence,
            "Acknowledge persistence result",
            basis.action(Control::AcknowledgePersistence).is_some(),
            false,
            [INSET, 148.0, acknowledge_width, 28.0],
            &mut buttons,
        )?);
        let decision_width = (width - acknowledge_width - 3.0 * GAP) / 3.0;
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
        for (index, &(control, text)) in decisions.iter().enumerate() {
            nodes.push(self.button(
                &basis,
                control,
                text,
                basis.action(control).is_some(),
                false,
                [
                    INSET + acknowledge_width + GAP + index as f32 * (decision_width + GAP),
                    148.0,
                    decision_width,
                    28.0,
                ],
                &mut buttons,
            )?);
        }

        let source_first = if source_count == 0 {
            0
        } else {
            self.source_offset + 1
        };
        let source_last = self
            .source_offset
            .saturating_add(SOURCES_PER_PAGE)
            .min(source_count);
        nodes.push(label(
            "sources",
            &format!(
                "Sources {source_first}-{source_last}/{source_count} | {}",
                feedback::source_status(basis.browser.status()),
            ),
            [INSET, 184.0, half, 20.0],
        )?);
        nodes.push(label(
            "open-status",
            &feedback::open_status(&basis.document_open),
            [INSET + half + GAP, 184.0, half, 20.0],
        )?);
        let source_width = (width - (SOURCES_PER_PAGE - 1) as f32 * GAP) / SOURCES_PER_PAGE as f32;
        for (slot, (index, entry)) in basis
            .sources()
            .skip(self.source_offset)
            .take(SOURCES_PER_PAGE)
            .enumerate()
        {
            let control = Control::OpenSource(index);
            nodes.push(self.button(
                &basis,
                control,
                &format!("Open {}", entry.relative_path()),
                basis.action(control).is_some(),
                false,
                [
                    INSET + slot as f32 * (source_width + GAP),
                    206.0,
                    source_width,
                    26.0,
                ],
                &mut buttons,
            )?);
        }
        let source_action_width = (width - 4.0 * GAP) / 5.0;
        for (slot, (control, text, enabled)) in [
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
        .enumerate()
        {
            nodes.push(self.button(
                &basis,
                control,
                text,
                enabled,
                false,
                [
                    INSET + slot as f32 * (source_action_width + GAP),
                    238.0,
                    source_action_width,
                    26.0,
                ],
                &mut buttons,
            )?);
        }

        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill().with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::role(UiColorRole::Surface))
        .with_children(nodes);
        let snapshot = UiAuthoringSnapshotBuilder::new(self.next_snapshot)
            .with_root(root)
            .build()
            .map_err(|error| error.to_string())?;
        let next_snapshot = self
            .next_snapshot
            .checked_add(1)
            .ok_or_else(|| "Workspace shell snapshot identities are exhausted".to_owned())?;
        let theme = UiTheme::new(environment.theme_revision().get(), Default::default())
            .map_err(|error| error.to_string())?;
        let frame = self
            .compiler
            .compile(&snapshot, environment, theme)
            .map_err(|error| error.to_string())?;
        self.next_snapshot = next_snapshot;
        if !frame.root_outcomes().iter().any(|outcome| {
            outcome.key() == &self.root && outcome.status() == UiRootStatus::Published
        }) {
            self.cancel_input()?;
            return Err("The workspace shell controls could not be published".into());
        }
        self.states.retain(|key, _| buttons.contains_key(key));
        self.frames.push(CompiledShell {
            model_revision: model.revision(),
            basis,
            frame: Arc::clone(&frame),
            generation: self.generation,
            offset: self.document_offset,
            source_offset: self.source_offset,
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
        rect: [f32; 4],
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
        let key = UiKey::scoped(
            self.root.clone(),
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
        let (background, foreground) = match state.visual_state(&button) {
            UiButtonVisualState::Disabled => (UiColorRole::Panel, UiColorRole::Disabled),
            UiButtonVisualState::Pressed | UiButtonVisualState::Focused => {
                (UiColorRole::Focus, UiColorRole::Surface)
            }
            UiButtonVisualState::Hovered => (UiColorRole::Accent, UiColorRole::Surface),
            UiButtonVisualState::Idle if selected => (UiColorRole::Accent, UiColorRole::Surface),
            UiButtonVisualState::Idle => (UiColorRole::Panel, UiColorRole::PrimaryText),
        };
        let node = button
            .authored_node(
                &self.root,
                UiStyle::absolute(rect[0], rect[1], rect[2], rect[3])
                    .with_overflow(UiOverflow::Clip),
                UiVisual::role(background),
                UiBrush::Role(foreground),
                TEXT_SIZE,
            )
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

fn label(key: &str, text: &str, rect: [f32; 4]) -> Result<UiNode, String> {
    Ok(UiNode::new(
        UiStyle::absolute(rect[0], rect[1], rect[2], rect[3]).with_overflow(UiOverflow::Clip),
    )
    .with_key(UiLocalKey::runtime("nara.shell.label", key).map_err(|error| error.to_string())?)
    .with_text(
        UiText::new(bounded_text(text, 180), TEXT_SIZE)
            .map_err(|error| error.to_string())?
            .with_overflow(UiTextOverflow::Clip),
    ))
}
