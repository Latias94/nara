use super::engine::Engine;
use anyhow::{Context as _, Result, ensure};
use nara_engine::tooling::{
    EditorAction, EditorDocumentMutation, EditorPersistenceAction, EditorReopenTarget,
};
use nara_gpui_engine_consumer::dialogue::{
    DialogueCommand, NodeBody, NodeId, NodeRecipe, OutputPort, preview::ConversationStep,
    workflow::DialoguePreview, workflow::DialogueSnapshot,
};
use open_gpui::{
    AnyElement, App, Context, Entity, IntoElement, MouseButton, Render, ScrollHandle, Subscription,
    Window, div, prelude::*, px, rgb,
};
use open_gpui_ui_components::{Button, Select, TextInput, Textarea, listbox::ListboxOption};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

#[path = "dialogue_panel/probe.rs"]
mod probe;
use probe::ObservedElement;
pub(super) use probe::{
    AcceptedFrame, ActivateCallback, Hit, PreviewPresentation, Probe, TextCallback,
};
#[path = "dialogue_panel/editor.rs"]
mod editor;
pub(super) use editor::DraftEditor;
#[path = "dialogue_panel/resources.rs"]
mod resources;
use resources::ResourceBrowser;

type Finish = Box<dyn FnOnce(&mut App) -> Result<()>>;
type Work = Box<dyn FnOnce(&mut Engine) -> Result<Finish>>;
pub(super) type Queue = Rc<RefCell<VecDeque<Work>>>;

pub(super) struct DialogueModel {
    pub snapshot: DialogueSnapshot,
    pub browser: nara_engine::tooling::EditorProjectBrowserView,
    pub browser_result: Option<nara_engine::tooling::EditorDocumentOpenResult>,
    status: String,
}

impl DialogueModel {
    pub fn publish_browser(
        &mut self,
        browser: nara_engine::tooling::EditorProjectBrowserView,
        result: Option<nara_engine::tooling::EditorDocumentOpenResult>,
        cx: &mut Context<Self>,
    ) {
        if self.browser != browser || self.browser_result != result {
            self.browser = browser;
            self.browser_result = result;
            cx.notify();
        }
    }
    pub fn publish(&mut self, snapshot: DialogueSnapshot, status: String, cx: &mut Context<Self>) {
        if self.snapshot != snapshot || self.status != status {
            self.snapshot = snapshot;
            self.status = status;
            cx.notify();
        }
    }
}

/// Two frontend Views over the original Session snapshot. All writable domain state stays there.
pub(super) struct DialoguePanel {
    pub model: Entity<DialogueModel>,
    pub editors: [Entity<DraftEditor>; 2],
    pub node_editors: [Entity<DraftEditor>; 2],
    pub resources: [Entity<ResourceBrowser>; 2],
    pub probes: [Rc<Probe>; 2],
    previews: [DialoguePreview; 2],
    preview_errors: [Option<String>; 2],
    preview_generations: [u64; 2],
    preview_scrolls: [ScrollHandle; 2],
    queue: Queue,
    _subscription: Option<Subscription>,
}

impl DialoguePanel {
    pub fn new(
        snapshot: DialogueSnapshot,
        browser: nara_engine::tooling::EditorProjectBrowserView,
        queue: Queue,
        composing: Rc<Cell<bool>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Self> {
        let initial_node = snapshot
            .view
            .entry()
            .context("the seeded Dialogue has no entry")?;
        let model = cx.new(|_| DialogueModel {
            snapshot: snapshot.clone(),
            browser,
            browser_result: None,
            status: "Ready".into(),
        });
        let probes: [_; 2] = std::array::from_fn(|index| {
            let probe = Rc::new(Probe::default());
            probe.view_index.set(index);
            probe
        });
        let mut create_editors = |property| {
            std::array::from_fn(|index| {
                cx.new(|cx| {
                    DraftEditor::new(
                        model.clone(),
                        queue.clone(),
                        initial_node,
                        probes[index].clone(),
                        composing.clone(),
                        property,
                        window,
                        cx,
                    )
                })
            })
        };
        let editors = create_editors(true);
        let node_editors = create_editors(false);
        let resources = std::array::from_fn(|index| {
            cx.new(|cx| {
                ResourceBrowser::new(model.clone(), queue.clone(), probes[index].clone(), cx)
            })
        });
        Ok(Self {
            model,
            editors,
            node_editors,
            resources,
            probes,
            previews: std::array::from_fn(|_| DialoguePreview::new(snapshot.clone())),
            preview_errors: [None, None],
            preview_generations: [0; 2],
            preview_scrolls: std::array::from_fn(|_| ScrollHandle::new()),
            queue,
            _subscription: None,
        })
    }

    pub fn mount(mut self, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            self._subscription = Some(cx.observe(&self.model, |_, _, cx| cx.notify()));
            self
        })
    }

    fn action(&self, index: usize, action: EditorAction) {
        let model = self.model.clone();
        let probe = self.probes[index].clone();
        self.queue.borrow_mut().push_back(Box::new(move |engine| {
            if probe.retired.get() {
                return Ok(Box::new(|_| Ok(())));
            }
            let outcome = engine.dialogue_action(action)?;
            let snapshot = engine.dialogue_snapshot()?;
            Ok(Box::new(move |cx| {
                model.update(cx, |model, cx| {
                    model.publish(snapshot, result_status(&outcome).into(), cx)
                });
                Ok(())
            }))
        }));
    }

    pub fn view_closed(&self, index: usize) -> bool {
        self.probes[index].retired.get()
    }

    pub fn close_view(&self, index: usize, window: &mut Window, cx: &mut App) -> Result<()> {
        ensure!(index < 2, "unknown Dialogue View");
        self.probes[index].retired.set(true);
        self.probes[index].accepted.borrow_mut().take();
        self.editors[index].update(cx, |editor, cx| editor.retire(cx));
        self.node_editors[index].update(cx, |editor, cx| editor.retire(cx));
        cx.notify(self.model.entity_id());
        window.refresh();
        Ok(())
    }

    pub fn close(&self, window: &mut Window, cx: &mut App) -> Result<()> {
        for index in 0..2 {
            self.close_view(index, window, cx)?;
        }
        Ok(())
    }

    pub fn verify_retired(&self, cx: &App) -> Result<()> {
        for index in 0..2 {
            ensure!(
                self.view_closed(index)
                    && self.editors[index].read(cx).is_retired()
                    && self.node_editors[index].read(cx).is_retired()
                    && self.probes[index].accepted.borrow().is_none(),
                "Dialogue View retained a live editor or accepted input publication"
            );
        }
        Ok(())
    }

    pub fn accepted(&self, cx: &App) -> Option<[AcceptedFrame; 2]> {
        let [Some(first), Some(second)] =
            std::array::from_fn(|index| self.accepted_view(index, cx))
        else {
            return None;
        };
        Some([first, second])
    }

    pub fn accepted_view(&self, index: usize, cx: &App) -> Option<AcceptedFrame> {
        self.probes[index]
            .accepted
            .borrow()
            .clone()
            .filter(|frame| {
                !self.view_closed(index)
                    && frame.snapshot == self.model.read(cx).snapshot
                    && frame.node == self.node_editors[index].read(cx).draft.node()
            })
    }

    fn preview(&self, index: usize, snapshot: &DialogueSnapshot, cx: &Context<Self>) -> AnyElement {
        let generation = self.preview_generations[index];
        let mut preview = div()
            .id(("dialogue-preview-scroll", index))
            .h(px(200.))
            .flex_none()
            .overflow_y_scroll()
            .track_scroll(&self.preview_scrolls[index])
            .p_2()
            .bg(rgb(0x191c1f))
            .flex()
            .flex_col()
            .gap_1()
            .child(ObservedElement::control(
                &self.probes[index],
                Hit::PreviewRestart,
                Button::new("dialogue-preview-restart", "Restart preview").on_activate(
                    cx.processor({
                        let basis = snapshot.clone();
                        move |this, _, _, cx| {
                            if !this.admits_preview(index, &basis, generation, cx) {
                                return;
                            }
                            this.previews[index] =
                                DialoguePreview::new(this.model.read(cx).snapshot.clone());
                            this.preview_generations[index] += 1;
                            this.preview_errors[index] = None;
                            cx.notify();
                        }
                    }),
                ),
            ));
        match self.previews[index].step(snapshot) {
            Ok(ConversationStep::Line { speaker, text, .. }) => {
                preview =
                    preview
                        .child(format!("{speaker}: {text}"))
                        .child(ObservedElement::control(
                            &self.probes[index],
                            Hit::PreviewNext,
                            Button::new("dialogue-preview-next", "Next").on_activate(cx.processor(
                                {
                                    let basis = snapshot.clone();
                                    move |this, _, _, cx| {
                                        if !this.admits_preview(index, &basis, generation, cx) {
                                            return;
                                        }
                                        this.preview_errors[index] = this.previews[index]
                                            .advance(&this.model.read(cx).snapshot)
                                            .err();
                                        this.preview_generations[index] += 1;
                                        cx.notify();
                                    }
                                },
                            )),
                        ));
            }
            Ok(ConversationStep::Choice {
                prompt, choices, ..
            }) => {
                preview = preview.child(prompt.to_owned());
                for choice in choices {
                    let choice_id = choice.id;
                    preview = preview.child(ObservedElement::control(
                        &self.probes[index],
                        Hit::PreviewChoice(choice_id),
                        Button::new(
                            format!("dialogue-preview-choice-{}", choice_id.0),
                            choice.label.clone(),
                        )
                        .on_activate(cx.processor({
                            let basis = snapshot.clone();
                            move |this, _, _, cx| {
                                if !this.admits_preview(index, &basis, generation, cx) {
                                    return;
                                }
                                this.preview_errors[index] = this.previews[index]
                                    .choose(&this.model.read(cx).snapshot, choice_id)
                                    .err();
                                this.preview_generations[index] += 1;
                                cx.notify();
                            }
                        })),
                    ));
                }
            }
            Ok(ConversationStep::End { text, .. }) => {
                preview = preview.child(format!("End: {text}"))
            }
            Err(error) => preview = preview.child(error),
        }
        preview
            .when_some(self.preview_errors[index].clone(), |element, error| {
                element.child(error)
            })
            .into_any_element()
    }

    fn admits_preview(
        &self,
        index: usize,
        basis: &DialogueSnapshot,
        generation: u64,
        cx: &App,
    ) -> bool {
        !self.view_closed(index)
            && self.preview_generations[index] == generation
            && self.model.read(cx).snapshot.document == basis.document
            && self.model.read(cx).snapshot.revision == basis.revision
    }

    fn column(&self, index: usize, snapshot: &DialogueSnapshot, cx: &Context<Self>) -> AnyElement {
        if self.view_closed(index) {
            return div()
                .flex_1()
                .child(format!(
                    "View {} closed; document and history remain open",
                    index + 1
                ))
                .into_any_element();
        }
        let probe = &self.probes[index];
        let node_editor = self.node_editors[index].read(cx);
        let node = node_editor.draft.node();
        let node_binding = node_editor.binding(cx);
        let node_live = node_editor.admits(node_binding, cx) && node_editor.draft.editable();
        let mut toolbar = div()
            .flex()
            .gap_1()
            .items_center()
            .child(format!("View {}", index + 1));
        for (hit, label, disabled) in [
            (Hit::Undo, "Undo", snapshot.history.undo_depth == 0),
            (Hit::Redo, "Redo", snapshot.history.redo_depth == 0),
            (Hit::Save, "Save", snapshot.persistence_pending),
            (
                Hit::Reopen,
                "Reopen",
                snapshot.persistence_pending || snapshot.dirty,
            ),
        ] {
            let basis = snapshot.clone();
            toolbar = toolbar.child(ObservedElement::control(
                probe,
                hit,
                Button::new(format!("dialogue-{hit:?}"), label)
                    .disabled(disabled)
                    .on_activate(cx.processor(move |this, _, _, _| {
                        if this.view_closed(index) {
                            return;
                        }
                        let action = match hit {
                            Hit::Undo => EditorAction::Mutate(EditorDocumentMutation::Undo {
                                document: basis.document,
                                base_revision: basis.revision,
                            }),
                            Hit::Redo => EditorAction::Mutate(EditorDocumentMutation::Redo {
                                document: basis.document,
                                base_revision: basis.revision,
                            }),
                            Hit::Save => EditorAction::Persistence(EditorPersistenceAction::Save {
                                document: basis.document,
                                base_revision: basis.revision,
                            }),
                            _ => EditorAction::Persistence(EditorPersistenceAction::Reopen {
                                target: EditorReopenTarget::OpenDocument {
                                    document: basis.document,
                                    base_revision: basis.revision,
                                },
                            }),
                        };
                        this.action(index, action);
                    })),
            ));
        }
        toolbar = toolbar.child(ObservedElement::control(
            probe,
            Hit::CloseView,
            Button::new("dialogue-close-view", "Close").on_activate(cx.processor(
                move |this, _, window, cx| {
                    if let Err(error) = this.close_view(index, window, cx) {
                        this.preview_errors[index] = Some(error.to_string());
                    }
                    cx.notify();
                },
            )),
        ));
        let mut create = div().flex().gap_1();
        for (hit, label, recipe) in [
            (Hit::CreateLine, "+ Line", NodeRecipe::Line),
            (Hit::CreateChoice, "+ Choice", NodeRecipe::Choice),
            (Hit::CreateEnd, "+ End", NodeRecipe::End),
        ] {
            let basis = snapshot.clone();
            create = create.child(ObservedElement::control(
                probe,
                hit,
                Button::new(format!("dialogue-{hit:?}"), label)
                    .disabled(snapshot.persistence_pending)
                    .on_activate(cx.processor(move |this, _, _, _| {
                        if !this.view_closed(index) {
                            this.action(
                                index,
                                EditorAction::Extension(basis.command(DialogueCommand::Add {
                                    recipe,
                                    position: [80., 280.],
                                })),
                            );
                        }
                    })),
            ));
        }
        let mut nodes = div().flex().flex_col().gap_1();
        for record in snapshot.view.nodes() {
            let id = record.id;
            let basis = snapshot.clone();
            let kind = match record.body.as_ref() {
                NodeBody::Line { .. } => "Line",
                NodeBody::Choice { .. } => "Choice",
                NodeBody::End { .. } => "End",
            };
            nodes = nodes.child(ObservedElement::control(
                probe,
                Hit::NodeSelect(id),
                Button::new(
                    format!("dialogue-node-{}", id.0),
                    format!(
                        "{kind} {}{}",
                        id.0,
                        if snapshot.view.entry() == Some(id) {
                            " · entry"
                        } else {
                            ""
                        }
                    ),
                )
                .selected(node_live && node == id)
                .on_activate(cx.processor(move |this, _, _, cx| {
                    if this.view_closed(index)
                        || this.model.read(cx).snapshot.revision != basis.revision
                    {
                        return;
                    }
                    this.node_editors[index].update(cx, |editor, cx| editor.select(id, cx));
                    cx.notify();
                })),
            ));
        }
        let mut node_actions = div().flex().gap_1();
        for (hit, label) in [
            (Hit::SetEntry, "Set entry"),
            (Hit::RemoveNode, "Delete node"),
        ] {
            let basis = snapshot.clone();
            let callback = cx.weak_entity();
            let handler: ActivateCallback = Rc::new(move |_, cx| {
                let _ = callback.update(cx, |this, cx| {
                    let editor = this.node_editors[index].read(cx);
                    if !editor.admits(node_binding, cx) || !editor.draft.editable() {
                        return;
                    }
                    let command = if hit == Hit::SetEntry {
                        DialogueCommand::SetEntry(Some(node))
                    } else {
                        DialogueCommand::Remove {
                            nodes: vec![node],
                            links: Vec::new(),
                        }
                    };
                    this.action(index, EditorAction::Extension(basis.command(command)));
                });
            });
            let activate = handler.clone();
            node_actions = node_actions.child(
                ObservedElement::control(
                    probe,
                    hit,
                    Button::new(format!("dialogue-{hit:?}"), label)
                        .disabled(!node_live || snapshot.persistence_pending)
                        .on_activate(move |_, window, cx| activate(window, cx)),
                )
                .with_activation(handler),
            );
        }
        let mut card = div()
            .flex()
            .flex_col()
            .gap_1()
            .child("Node / local selection")
            .child(nodes)
            .child(node_actions);
        if matches!(
            snapshot.view.node(node).map(|record| record.body.as_ref()),
            Some(NodeBody::Line { .. })
        ) {
            let basis = snapshot.clone();
            card = card.child(ObservedElement::control(
                probe,
                Hit::NextTarget,
                Select::new("dialogue-next-target", "Next target · applies immediately")
                    .disabled(!node_live || snapshot.persistence_pending)
                    .selected(Some(
                        snapshot
                            .view
                            .target(OutputPort::Next { node })
                            .map_or_else(|| "none".into(), |node| node.0.to_string()),
                    ))
                    .option(ListboxOption::new("none", "Unconnected"))
                    .options(snapshot.view.nodes().map(|record| {
                        ListboxOption::new(record.id.0.to_string(), format!("Node {}", record.id.0))
                    }))
                    .on_select(cx.processor(
                        move |this,
                              selected: open_gpui_ui_components::select::SelectSelection,
                              _,
                              cx| {
                            let editor = this.node_editors[index].read(cx);
                            if !editor.admits(node_binding, cx) || !editor.draft.editable() {
                                return;
                            }
                            this.action(
                                index,
                                EditorAction::Extension(basis.command(
                                    DialogueCommand::SetTarget {
                                        port: OutputPort::Next { node },
                                        target: selected.value().parse().ok().map(NodeId),
                                    },
                                )),
                            );
                        },
                    )),
            ));
        }
        card = card.child(self.node_editors[index].clone());
        let contents = div().size_full().flex().flex_col().gap_1().p_2().bg(rgb(0x202326))
            .child(toolbar).child(create)
            .child(ObservedElement::control(probe, Hit::GraphUnavailable,
                div().p_1().child("Graph canvas and drag are unavailable; edit the original document with the controls below.")))
            .child(div().flex_1().min_h_0().flex().gap_2()
                .child(div().id("dialogue-node-scroll").flex_1().min_w_0().overflow_y_scroll().child(card))
                .child(div().id("dialogue-properties-scroll").flex_1().min_w_0().overflow_y_scroll().child(self.editors[index].clone())))
            .child(self.preview(index, snapshot, cx));
        let contents = contents.child(self.resources[index].clone());
        ObservedElement::frame(
            probe,
            snapshot.clone(),
            node,
            div()
                .id(("dialogue-column", index))
                .flex_1()
                .min_w_0()
                .child(contents),
        )
        .with_preview(
            PreviewPresentation::capture(self.previews[index].step(snapshot)),
            self.preview_errors[index].clone(),
        )
        .into_any_element()
    }
}

pub(super) fn result_status(outcome: &nara_engine::tooling::EditorActionOutcome) -> &'static str {
    use nara_engine::tooling::{
        EditorActionOutcome, EditorPersistenceRequestResult, EditorProjectBrowserActionOutcome,
    };
    match outcome {
        EditorActionOutcome::Workspace(report) if report.applied => "Edit applied",
        EditorActionOutcome::Workspace(report) if report.extension_outcome.is_some() => "No change",
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. }) => {
            "Persistence pending"
        }
        EditorActionOutcome::Persistence(_) => "Persistence refused; review current source",
        EditorActionOutcome::ProjectBrowser(outcome) => match outcome {
            EditorProjectBrowserActionOutcome::RefreshRequested
            | EditorProjectBrowserActionOutcome::RefreshAlreadyPending => {
                "Refreshing project index"
            }
            EditorProjectBrowserActionOutcome::Preparing { .. } => "Opening selected document",
            EditorProjectBrowserActionOutcome::Activated { .. } => "Document activated",
            EditorProjectBrowserActionOutcome::OpenCancellationRequested { .. } => {
                "Cancelling document open"
            }
            EditorProjectBrowserActionOutcome::OpenAcknowledged { .. } => {
                "Document open result received"
            }
            EditorProjectBrowserActionOutcome::Rejected { .. } => {
                "Project action could not be completed"
            }
        },
        _ => "Edit refused; review current source",
    }
}

impl Render for DialoguePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.model.read(cx).snapshot.clone();
        let page = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x181a1d))
            .text_color(rgb(0xe2e4e7))
            .text_size(px(13.))
            .font_family("Segoe UI")
            .gap_1()
            .child(div().h(px(40.)).flex_none().p_2().child(format!(
                "Nara / Dialogue · {} · {}",
                if snapshot.dirty { "Modified" } else { "Saved" },
                self.model.read(cx).status
            )))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap_2()
                    .children((0..2).map(|index| self.column(index, &snapshot, cx))),
            );
        open_gpui_ui_components::theme::ThemeScope::new(
            "nara-dialogue-theme",
            open_gpui_ui_components::theme::ThemeContext::dark(),
            page,
        )
    }
}
