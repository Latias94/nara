use super::*;
use nara_core::{DocumentRevision, DocumentSourceId};
use nara_engine::tooling::{EditorActionOutcome, EditorDocumentId, EditorExtensionCommand};
use nara_gpui_engine_consumer::dialogue::workflow::{
    ConflictResolution, DialogueDraft, DraftState,
};
use open_gpui_ui_components::gpui_adapter::OverlayOpenIntent;
use probe::{ActivateCallback, TextCallback};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct EditorBinding {
    epoch: usize,
    node: NodeId,
    source: DocumentSourceId,
    revision: DocumentRevision,
    incarnation: Option<u64>,
}

/// Selecting a live node requires the View and document, not the old draft's node lifetime.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ViewBinding {
    epoch: usize,
    document: EditorDocumentId,
    source: DocumentSourceId,
}

pub(crate) struct DraftEditor {
    pub draft: DialogueDraft,
    model: Entity<DialogueModel>,
    queue: Queue,
    probe: Rc<Probe>,
    property: bool,
    retired: bool,
    epoch: usize,
    composing: Rc<Cell<bool>>,
    input_error: Option<String>,
    parked: BTreeMap<NodeId, DialogueDraft>,
    pub selector_open: bool,
    _subscription: Subscription,
}

impl DraftEditor {
    pub fn new(
        model: Entity<DialogueModel>,
        queue: Queue,
        node: NodeId,
        probe: Rc<Probe>,
        composing: Rc<Cell<bool>>,
        property: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = DialogueDraft::new(model.read(cx).snapshot.clone(), node)
            .expect("a mounted Dialogue node exists in its original observation");
        let weak = cx.weak_entity();
        let subscription = window.observe(&model, cx, move |model, _, cx| {
            let snapshot = model.read(cx).snapshot.clone();
            let _ = weak.update(cx, |this, cx| {
                let previous = this.draft.basis().revision;
                let previous_state = this.draft.state();
                if !this.draft.basis().same_source(&snapshot) {
                    this.parked.clear();
                }
                this.draft.observe(&snapshot);
                if previous != this.draft.basis().revision
                    || previous_state != DraftState::Retired
                        && this.draft.state() == DraftState::Retired
                {
                    this.epoch += 1;
                    this.selector_open = false;
                }
                for draft in this.parked.values_mut() {
                    draft.observe(&snapshot);
                }
                this.parked.retain(|node, draft| {
                    snapshot.view.node(*node).is_some() && draft.state() != DraftState::Retired
                });
                cx.notify();
            });
        });
        Self {
            draft,
            model,
            queue,
            probe,
            property,
            composing,
            input_error: None,
            parked: BTreeMap::new(),
            selector_open: false,
            retired: false,
            epoch: 0,
            _subscription: subscription,
        }
    }

    pub fn retire(&mut self, cx: &mut Context<Self>) {
        self.retired = true;
        self.parked.clear();
        self.selector_open = false;
        self.epoch += 1;
        cx.notify();
    }

    pub fn is_retired(&self) -> bool {
        self.retired
    }

    pub fn select(&mut self, node: NodeId, cx: &mut Context<Self>) {
        if self.retired || self.probe.retired.get() {
            return;
        }
        if self.draft.state() == DraftState::Pending {
            return;
        }
        let snapshot = self.model.read(cx).snapshot.clone();
        if snapshot.view.node(node).is_none() {
            return;
        }
        if node == self.draft.node()
            && self.draft.state() != DraftState::Retired
            && self.draft.basis().same_source(&snapshot)
            && self.draft.basis().view.node_incarnation(node)
                == snapshot.view.node_incarnation(node)
        {
            self.selector_open = false;
            cx.notify();
            return;
        }
        if let Some(draft) = self
            .parked
            .remove(&node)
            .filter(|draft| {
                draft.state() != DraftState::Retired
                    && draft.basis().same_source(&snapshot)
                    && draft.basis().view.node_incarnation(node)
                        == snapshot.view.node_incarnation(node)
            })
            .or_else(|| DialogueDraft::new(snapshot, node))
        {
            let previous = std::mem::replace(&mut self.draft, draft);
            if previous.state() != DraftState::Retired && previous.node() != node {
                self.parked.insert(previous.node(), previous);
            }
            self.epoch += 1;
            self.selector_open = false;
            self.input_error = None;
            cx.notify();
        }
    }

    fn view_binding(&self, cx: &App) -> ViewBinding {
        let snapshot = &self.model.read(cx).snapshot;
        ViewBinding {
            epoch: self.epoch,
            document: snapshot.document,
            source: snapshot.revision.source_id(),
        }
    }

    fn admits_view(&self, binding: ViewBinding, cx: &App) -> bool {
        !self.retired && !self.probe.retired.get() && self.view_binding(cx) == binding
    }

    pub(super) fn binding(&self, cx: &App) -> EditorBinding {
        EditorBinding {
            epoch: self.epoch,
            node: self.draft.node(),
            source: self.draft.basis().revision.source_id(),
            revision: self.model.read(cx).snapshot.revision,
            incarnation: self.draft.basis().view.node_incarnation(self.draft.node()),
        }
    }

    pub(super) fn admits(&self, binding: EditorBinding, cx: &App) -> bool {
        let snapshot = &self.model.read(cx).snapshot;
        !self.retired
            && !self.probe.retired.get()
            && self.draft.has_live_node(snapshot)
            && self.binding(cx) == binding
            && self.draft.basis().document == snapshot.document
            && snapshot.revision.source_id() == binding.source
            && snapshot.view.node_incarnation(binding.node) == binding.incarnation
    }

    fn text_callback(
        &self,
        cx: &Context<Self>,
        update: impl Fn(&mut nara_gpui_engine_consumer::dialogue::workflow::NodeDraft, String) + 'static,
    ) -> TextCallback {
        let binding = self.binding(cx);
        let weak = cx.weak_entity();
        Rc::new(move |text, _, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.admits(binding, cx) && this.draft.edit(|draft| update(draft, text)) {
                    cx.notify();
                }
            });
        })
    }

    fn apply_callback(&self, cx: &Context<Self>, position: bool) -> ActivateCallback {
        let binding = self.binding(cx);
        let weak = cx.weak_entity();
        Rc::new(move |_, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.admits(binding, cx) {
                    this.apply(position, cx);
                }
            });
        })
    }

    fn apply(&mut self, position: bool, cx: &mut Context<Self>) {
        if self.retired {
            return;
        }
        if self.composing.get() {
            self.input_error = Some("Finish IME composition before Apply".into());
            cx.notify();
            return;
        }
        self.input_error = None;
        let snapshot = self.model.read(cx).snapshot.clone();
        let Ok(command) = self.draft.prepare(&snapshot, position) else {
            cx.notify();
            return;
        };
        let weak = cx.weak_entity();
        let model = self.model.clone();
        let epoch = self.epoch;
        let probe = self.probe.clone();
        self.queue.borrow_mut().push_back(Box::new(move |engine| {
            if probe.retired.get() {
                return Ok(Box::new(|_| Ok(())));
            }
            let outcome = engine.dialogue_action(EditorAction::Extension(command.clone()))?;
            let snapshot = engine.dialogue_snapshot()?;
            Ok(Box::new(move |cx| {
                let settled = weak.update(cx, |this, cx| {
                    if this.retired || this.epoch != epoch {
                        return Ok(());
                    }
                    this.finish(&command, &outcome, &snapshot, cx)
                });
                if let Ok(result) = settled {
                    result?;
                }
                model.update(cx, |model, cx| {
                    model.publish(snapshot, result_status(&outcome).into(), cx)
                });
                Ok(())
            }))
        }));
        cx.notify();
    }

    fn finish(
        &mut self,
        command: &EditorExtensionCommand,
        outcome: &EditorActionOutcome,
        snapshot: &DialogueSnapshot,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.draft
            .settle(command, outcome, snapshot)
            .map_err(anyhow::Error::msg)?;
        self.epoch += 1;
        self.selector_open = false;
        cx.notify();
        Ok(())
    }
}

impl Render for DraftEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.retired {
            return div()
                .child("View closed; document remains open")
                .into_any_element();
        }
        let snapshot = self.model.read(cx).snapshot.clone();
        let value = self.draft.value().clone();
        let node = self.draft.node();
        let property = self.property;
        let disabled = !self.draft.editable();
        let stale = matches!(
            self.draft.state(),
            DraftState::Conflict | DraftState::Retired
        );
        let binding = self.binding(cx);
        let view_binding = self.view_binding(cx);
        let prefix = format!(
            "dialogue-{}-{}-{}",
            self.probe.view_index.get(),
            if property { "properties" } else { "node" },
            node.0
        );
        let id = format!(
            "{prefix}-{}-{:?}",
            self.epoch,
            snapshot.revision.source_id()
        );
        let mut fields = div()
            .id(id)
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(
                cx.listener(move |this, event: &open_gpui::KeyDownEvent, _, cx| {
                    if this.admits(binding, cx)
                        && !this.selector_open
                        && event.keystroke.key == "escape"
                        && this.draft.cancel(&this.model.read(cx).snapshot)
                    {
                        this.epoch += 1;
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            );
        if property {
            fields = fields.child(ObservedElement::control(
                &self.probe,
                Hit::NodeSelector,
                Select::new(format!("{prefix}-selector"), "Properties / node")
                    .open(self.selector_open)
                    .on_open_change(cx.processor(move |this, intent: OverlayOpenIntent, _, cx| {
                        if this.admits_view(view_binding, cx) {
                            this.selector_open = intent.desired_open();
                            cx.notify();
                        }
                    }))
                    .selected(
                        (self.draft.state() != DraftState::Retired).then(|| node.0.to_string()),
                    )
                    .placeholder("Select a live node")
                    .disabled(self.draft.state() == DraftState::Pending)
                    .options(snapshot.view.nodes().map(|node| {
                        ListboxOption::new(
                            node.id.0.to_string(),
                            format!(
                                "Node {} · {:?}",
                                node.id.0,
                                match node.body.as_ref() {
                                    NodeBody::Line { .. } => "Line",
                                    NodeBody::Choice { .. } => "Choice",
                                    _ => "End",
                                }
                            ),
                        )
                    }))
                    .on_select(cx.processor(
                        move |this,
                              selection: open_gpui_ui_components::select::SelectSelection,
                              _,
                              cx| {
                            if this.admits_view(view_binding, cx)
                                && let Ok(node) = selection.value().parse::<u64>()
                            {
                                this.select(NodeId(node), cx);
                            }
                        },
                    )),
            ));
        }
        if matches!(
            snapshot.view.node(node).map(|node| node.body.as_ref()),
            Some(NodeBody::Line { .. })
        ) {
            let callback = self.text_callback(cx, |draft, text| draft.speaker = text);
            let handler = callback.clone();
            fields = fields.child(
                ObservedElement::control(
                    &self.probe,
                    if property {
                        Hit::Speaker
                    } else {
                        Hit::NativeSpeaker(node)
                    },
                    TextInput::new("dialogue-speaker", "Speaker")
                        .value(value.speaker.clone())
                        .read_only(disabled)
                        .on_change(move |text, window, cx| handler(text, window, cx)),
                )
                .with_text(callback),
            );
        }
        let rows = if property {
            3
        } else {
            value.text.lines().count().clamp(2, 6)
        };
        let text_callback = self.text_callback(cx, |draft, text| draft.text = text);
        let handler = text_callback.clone();
        fields = fields.child(
            ObservedElement::control(
                &self.probe,
                if property {
                    Hit::Text
                } else {
                    Hit::NativeText(node)
                },
                Textarea::new("dialogue-text", "Dialogue text / prompt")
                    .value(value.text.clone())
                    .rows(rows)
                    .read_only(disabled)
                    .invalid(self.draft.error().is_some())
                    .busy(self.draft.state() == DraftState::Pending)
                    .on_change(move |text, window, cx| handler(text, window, cx)),
            )
            .with_text(text_callback),
        );
        if matches!(
            snapshot.view.node(node).map(|node| node.body.as_ref()),
            Some(NodeBody::Choice { .. })
        ) {
            for (index, choice) in value.choices.iter().enumerate() {
                let choice_key = choice
                    .id
                    .map_or_else(|| format!("new-{index}"), |id| id.0.to_string());
                let mut target =
                    Select::new(format!("{prefix}-choice-target-{choice_key}"), "Target")
                        .disabled(disabled)
                        .selected(Some(
                            choice
                                .target
                                .map_or_else(|| "none".into(), |node| node.0.to_string()),
                        ))
                        .option(ListboxOption::new("none", "Unconnected"))
                        .options(snapshot.view.nodes().map(|node| {
                            ListboxOption::new(node.id.0.to_string(), format!("Node {}", node.id.0))
                        }))
                        .on_select(cx.processor(
                            move |this,
                                  selected: open_gpui_ui_components::select::SelectSelection,
                                  _,
                                  cx| {
                                if !this.admits(binding, cx) {
                                    return;
                                }
                                this.draft.edit(|draft| {
                                    if let Some(choice) = draft.choices.get_mut(index) {
                                        choice.target =
                                            selected.value().parse::<u64>().ok().map(NodeId);
                                    }
                                });
                                cx.notify();
                            },
                        ));
                target = target.placeholder("Target");
                let label_callback = self.text_callback(cx, move |draft, text| {
                    if let Some(choice) = draft.choices.get_mut(index) {
                        choice.label = text;
                    }
                });
                fields = fields.child(
                    div()
                        .id(format!("dialogue-choice-{choice_key}"))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(ObservedElement::control(
                            &self.probe,
                            if property {
                                Hit::ChoiceLabel(index)
                            } else {
                                Hit::NativeChoiceLabel(node, index)
                            },
                            Textarea::new("choice-label", "Choice label")
                                .value(choice.label.clone())
                                .rows(1)
                                .read_only(disabled)
                                .on_change(move |text, window, cx| {
                                    label_callback(text, window, cx)
                                }),
                        ))
                        .child(
                            div()
                                .flex()
                                .gap_1()
                                .child(ObservedElement::control(
                                    &self.probe,
                                    if property {
                                        Hit::ChoiceTarget(index)
                                    } else {
                                        Hit::NativeChoiceTarget(node, index)
                                    },
                                    target,
                                ))
                                .child(ObservedElement::control(
                                    &self.probe,
                                    if property {
                                        Hit::ChoiceUp(index)
                                    } else {
                                        Hit::NativeChoiceUp(node, index)
                                    },
                                    Button::new("choice-up", "↑")
                                        .disabled(disabled || index == 0)
                                        .on_activate(cx.processor(move |this, _, _, cx| {
                                            if this.admits(binding, cx) && index > 0 {
                                                this.draft.edit(|draft| {
                                                    draft.choices.swap(index - 1, index)
                                                });
                                                this.epoch += 1;
                                            }
                                            cx.notify();
                                        })),
                                ))
                                .child(ObservedElement::control(
                                    &self.probe,
                                    if property {
                                        Hit::ChoiceRemove(index)
                                    } else {
                                        Hit::NativeChoiceRemove(node, index)
                                    },
                                    Button::new("choice-remove", "Remove")
                                        .disabled(disabled)
                                        .on_activate(cx.processor(move |this, _, _, cx| {
                                            if !this.admits(binding, cx) {
                                                return;
                                            }
                                            this.draft.edit(|draft| {
                                                draft.choices.remove(index);
                                            });
                                            this.epoch += 1;
                                            cx.notify();
                                        })),
                                )),
                        ),
                );
            }
            fields = fields.child(ObservedElement::control(
                &self.probe,
                if property {
                    Hit::AddChoice
                } else {
                    Hit::NativeAddChoice(node)
                },
                Button::new("dialogue-add-choice", "Add choice")
                    .disabled(disabled)
                    .on_activate(cx.processor(move |this, _, _, cx| {
                        if !this.admits(binding, cx) {
                            return;
                        }
                        this.draft.edit(|draft| {
                            draft
                                .choices
                                .push(nara_gpui_engine_consumer::dialogue::ChoiceDraft {
                                    id: None,
                                    label: "New choice".into(),
                                    target: None,
                                })
                        });
                        this.epoch += 1;
                        cx.notify();
                    })),
            ));
        }
        if property {
            fields = fields
                .child(div().flex().gap_1().children((0..2).map(|axis| {
                    let callback =
                        self.text_callback(cx, move |draft, text| draft.position[axis] = text);
                    let handler = callback.clone();
                    ObservedElement::control(
                        &self.probe,
                        if axis == 0 { Hit::X } else { Hit::Y },
                        TextInput::new(
                            if axis == 0 {
                                "dialogue-x"
                            } else {
                                "dialogue-y"
                            },
                            if axis == 0 {
                                "Position X"
                            } else {
                                "Position Y"
                            },
                        )
                        .value(value.position[axis].clone())
                        .read_only(disabled)
                        .invalid(self.draft.error().is_some())
                        .on_change(move |text, window, cx| handler(text, window, cx)),
                    )
                    .with_text(callback)
                })))
                .child({
                    let callback = self.apply_callback(cx, true);
                    let handler = callback.clone();
                    ObservedElement::control(
                        &self.probe,
                        Hit::ApplyPosition,
                        Button::new("dialogue-apply-position", "Apply position")
                            .disabled(disabled || stale)
                            .on_activate(move |_, window, cx| handler(window, cx)),
                    )
                    .with_activation(callback)
                });
        }
        let apply_callback = self.apply_callback(cx, false);
        let handler = apply_callback.clone();
        let actions = div()
            .flex()
            .gap_1()
            .child(
                ObservedElement::control(
                    &self.probe,
                    if property {
                        Hit::Apply
                    } else {
                        Hit::NativeApply(node)
                    },
                    Button::new("dialogue-apply", "Apply")
                        .disabled(disabled || stale)
                        .on_activate(move |_, window, cx| handler(window, cx)),
                )
                .with_activation(apply_callback),
            )
            .child(ObservedElement::control(
                &self.probe,
                if property {
                    Hit::Cancel
                } else {
                    Hit::NativeCancel(node)
                },
                Button::new("dialogue-cancel", "Cancel")
                    .disabled(self.draft.state() == DraftState::Pending)
                    .on_activate(cx.processor(move |this, _, _, cx| {
                        if this.admits(binding, cx)
                            && this.draft.cancel(&this.model.read(cx).snapshot)
                        {
                            this.epoch += 1;
                            this.input_error = None;
                            this.selector_open = false;
                        }
                        cx.notify();
                    })),
            ))
            .when(property, |element| {
                element.child(ObservedElement::control(
                    &self.probe,
                    Hit::ReadOnly,
                    Button::new(
                        "dialogue-read-only",
                        if self.draft.read_only() {
                            "Unlock"
                        } else {
                            "Read only"
                        },
                    )
                    .on_activate(cx.processor(move |this, _, _, cx| {
                        if !this.admits(binding, cx) {
                            return;
                        }
                        this.draft.set_read_only(!this.draft.read_only());
                        cx.notify();
                    })),
                ))
            });
        fields = fields.child(actions);
        let conflicts = self.draft.conflicts();
        for (index, conflict) in conflicts.iter().enumerate() {
            let field = conflict.field;
            let mut comparison = div()
                .p_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(format!("{field:?} conflict"))
                .child(format!("Current: {}", conflict.current))
                .child(format!("Draft: {}", conflict.draft));
            for (resolution, label) in [
                (ConflictResolution::Current, "Use Current"),
                (ConflictResolution::Draft, "Use Draft"),
            ] {
                comparison = comparison.child(ObservedElement::control(
                    &self.probe,
                    if resolution == ConflictResolution::Current {
                        Hit::ResolveCurrent(property, index)
                    } else {
                        Hit::ResolveDraft(property, index)
                    },
                    Button::new(format!("{prefix}-resolve-{index}-{resolution:?}"), label)
                        .disabled(disabled || self.draft.basis().revision != snapshot.revision)
                        .on_activate(cx.processor(move |this, _, _, cx| {
                            if this.admits(binding, cx)
                                && this.draft.resolve(
                                    field,
                                    resolution,
                                    &this.model.read(cx).snapshot,
                                )
                            {
                                this.epoch += 1;
                                cx.notify();
                            }
                        })),
                ));
            }
            fields = fields.child(comparison);
        }
        if self.draft.state() == DraftState::Conflict
            && (conflicts.is_empty() || self.draft.basis().revision != snapshot.revision)
        {
            fields = fields.child(ObservedElement::control(
                &self.probe,
                if property {
                    Hit::Review
                } else {
                    Hit::NativeReview(node)
                },
                Button::new("dialogue-review", "Review draft on current source").on_activate(
                    cx.processor(move |this, _, _, cx| {
                        if !this.admits(binding, cx) {
                            return;
                        }
                        this.draft.review(&this.model.read(cx).snapshot);
                        this.epoch += 1;
                        cx.notify();
                    }),
                ),
            ));
        }
        if self.draft.state() == DraftState::Retired {
            let target = snapshot
                .view
                .node(node)
                .map(|node| node.id)
                .or_else(|| snapshot.view.entry())
                .or_else(|| snapshot.view.nodes().next().map(|node| node.id));
            fields = fields.child(ObservedElement::control(
                &self.probe,
                if property {
                    Hit::Rebind
                } else {
                    Hit::NativeRebind(node)
                },
                Button::new("dialogue-rebind", "Open available node")
                    .disabled(target.is_none())
                    .on_activate(cx.processor(move |this, _, _, cx| {
                        if this.admits_view(view_binding, cx)
                            && let Some(node) = target
                        {
                            this.select(node, cx);
                        }
                    })),
            ));
        }
        fields = fields.child(
            self.input_error
                .clone()
                .or_else(|| self.draft.error().map(str::to_owned))
                .unwrap_or_else(|| format!("{:?}", self.draft.state())),
        );
        fields.into_any_element()
    }
}
