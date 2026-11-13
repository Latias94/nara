use super::*;
use nara_engine::tooling::{
    EditorProjectBrowserAction, EditorProjectBrowserLocalAction, EditorProjectBrowserState,
    EditorProjectBrowserView,
};
use std::sync::Arc;

pub(crate) struct ResourceBrowser {
    state: EditorProjectBrowserState,
    model: Entity<DialogueModel>,
    queue: Queue,
    probe: Rc<Probe>,
    epoch: u64,
    status: String,
    pub expanded: bool,
    open_request: Option<nara_engine::tooling::EditorDocumentOpenId>,
    open_queued: bool,
    _subscription: Subscription,
}

impl ResourceBrowser {
    pub fn new(
        model: Entity<DialogueModel>,
        queue: Queue,
        probe: Rc<Probe>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&model, |this, model, cx| {
            let model = model.read(cx);
            this.synchronize(&model.browser);
            if let Some(result) = model.browser_result
                && this.open_request == Some(result.request())
            {
                this.status = match result {
                    nara_engine::tooling::EditorDocumentOpenResult::Opened { .. } => {
                        "Document opened".into()
                    }
                    nara_engine::tooling::EditorDocumentOpenResult::Cancelled { .. } => {
                        "Open cancelled".into()
                    }
                    nara_engine::tooling::EditorDocumentOpenResult::Rejected { .. } => {
                        "Document could not be opened".into()
                    }
                };
                this.open_request = None;
            }
            cx.notify();
        });
        let mut browser = Self {
            state: EditorProjectBrowserState::new(),
            model,
            queue,
            probe,
            epoch: 0,
            status: "Find project resources. Only supported documents can be opened here.".into(),
            expanded: false,
            open_request: None,
            open_queued: false,
            _subscription: subscription,
        };
        browser.synchronize(&browser.model.read(cx).browser.clone());
        browser
    }

    fn synchronize(&mut self, original: &EditorProjectBrowserView) {
        if self.state.view().snapshot_generation() != original.snapshot_generation() {
            self.epoch += 1;
        }
        self.state.synchronize(
            original.status(),
            original.shared_snapshot(),
            original.failure().cloned(),
            original.scan_progress(),
            original.has_pending_work(),
            original.work_observed(),
        );
    }

    pub fn view(&self) -> EditorProjectBrowserView {
        self.state.view()
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    fn toggle(&self, cx: &Context<Self>) -> AnyElement {
        ObservedElement::control(
            &self.probe,
            Hit::ResourceToggle,
            Button::new("dialogue-resource-toggle", "Project resources / documents")
                .selected(self.expanded)
                .on_activate(cx.processor(|this, _, _, cx| {
                    if !this.probe.retired.get() {
                        this.expanded = !this.expanded;
                        cx.notify();
                    }
                })),
        )
        .into_any_element()
    }

    fn admits(&self, generation: Option<u64>, epoch: u64, cx: &App) -> bool {
        !self.probe.retired.get()
            && self.epoch == epoch
            && self.model.read(cx).browser.snapshot_generation() == generation
    }

    fn enqueue(&mut self, path: Option<String>, generation: Option<u64>, cx: &Context<Self>) {
        if self.open_queued || self.open_request.is_some() {
            return;
        }
        self.open_queued = true;
        let weak = cx.weak_entity();
        let probe = self.probe.clone();
        self.queue.borrow_mut().push_back(Box::new(move |engine| {
            if probe.retired.get() {
                return Ok(Box::new(|_| Ok(())));
            }
            let (status, pending) = if let Some(path) = path {
                engine.dialogue_open_resource(
                    generation.context("resource has no source generation")?,
                    &path,
                )?
            } else {
                let outcome = engine.dialogue_action(EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Refresh,
                ))?;
                println!("NARA_GPUI_RESOURCE_REFRESH {outcome:?}");
                let status = match outcome {
                    nara_engine::tooling::EditorActionOutcome::ProjectBrowser(
                        nara_engine::tooling::EditorProjectBrowserActionOutcome::RefreshRequested
                        | nara_engine::tooling::EditorProjectBrowserActionOutcome::RefreshAlreadyPending,
                    ) => "Refreshing project index",
                    _ => "Project index could not be refreshed",
                };
                (status.into(), None)
            };
            Ok(Box::new(move |cx| {
                let _ = weak.update(cx, |this, cx| {
                    if !this.probe.retired.get() {
                        this.status = status;
                        this.open_request = pending;
                        this.open_queued = false;
                        cx.notify();
                    }
                });
                Ok(())
            }))
        }));
    }
}

impl Render for ResourceBrowser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.probe.retired.get() {
            return div().into_any_element();
        }
        if self.state.advance_query(256).pending() {
            cx.notify();
        }
        if !self.expanded {
            return div().flex_none().child(self.toggle(cx)).into_any_element();
        }
        let view = self.view();
        let generation = view.snapshot_generation();
        let epoch = self.epoch;
        let mut page = div()
            .id("dialogue-resource-browser")
            .h(px(260.))
            .flex_none()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .bg(rgb(0x191c1f))
            .child(self.toggle(cx))
            .child(ObservedElement::control(
                &self.probe,
                Hit::ResourceQuery,
                TextInput::new("dialogue-resource-query", "Find by project path / 中文")
                    .value(view.path_filter().to_owned())
                    .on_change(cx.processor(move |this, value: String, _, cx| {
                        if this.probe.retired.get()
                            || this.model.read(cx).browser.snapshot_generation() != generation
                        {
                            return;
                        }
                        if this.state.apply_local_action(
                            EditorProjectBrowserLocalAction::SetPathFilter {
                                value: Arc::from(value),
                            },
                        ) {
                            this.epoch += 1;
                            cx.notify();
                        }
                    })),
            ))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(ObservedElement::control(
                        &self.probe,
                        Hit::ResourceRefresh,
                        Button::new("dialogue-resource-refresh", "Refresh index")
                            .disabled(self.open_queued || self.open_request.is_some())
                            .on_activate(cx.processor(move |this, _, _, cx| {
                                if this.admits(generation, epoch, cx) {
                                    this.enqueue(None, generation, cx);
                                }
                            })),
                    ))
                    .child(ObservedElement::control(
                        &self.probe,
                        Hit::ResourceOpen,
                        Button::new("dialogue-resource-open", "Open document")
                            .disabled(
                                self.open_queued
                                    || self.open_request.is_some()
                                    || !view
                                        .selected_entry()
                                        .is_some_and(|entry| entry.open_capability().is_document()),
                            )
                            .on_activate(cx.processor(move |this, _, _, cx| {
                                let view = this.view();
                                if this.admits(generation, epoch, cx)
                                    && let Some(entry) = view.selected_entry()
                                    && entry.open_capability().is_document()
                                {
                                    this.enqueue(
                                        Some(entry.relative_path().to_owned()),
                                        generation,
                                        cx,
                                    );
                                }
                            })),
                    )),
            );
        let mut rows = div()
            .id("dialogue-resource-rows")
            .flex_1()
            .min_h(px(56.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for (index, row) in view.rows().iter().take(48).enumerate() {
            let path = row.relative_path().to_owned();
            let directory = row.kind() == nara_engine::tooling::EditorProjectSourceKind::Directory;
            rows = rows.child(ObservedElement::control(
                &self.probe,
                Hit::ResourceRow(index),
                Button::new(format!("dialogue-resource-{index}"), path.clone())
                    .selected(view.selected_path() == Some(path.as_str()))
                    .on_activate(cx.processor(move |this, _, _, cx| {
                        if !this.admits(generation, epoch, cx) {
                            return;
                        }
                        this.state
                            .apply_local_action(EditorProjectBrowserLocalAction::Select {
                                relative_path: Some(Arc::from(path.as_str())),
                            });
                        if directory {
                            this.state.apply_local_action(
                                EditorProjectBrowserLocalAction::ToggleExpanded {
                                    relative_path: Arc::from(path.as_str()),
                                },
                            );
                        }
                        this.epoch += 1;
                        cx.notify();
                    })),
            ));
        }
        let index_status = match view.status() {
            nara_engine::tooling::EditorProjectBrowserStatus::NotRequested => "Index not loaded",
            nara_engine::tooling::EditorProjectBrowserStatus::Scanning => {
                "Refreshing project index"
            }
            nara_engine::tooling::EditorProjectBrowserStatus::Ready => "Ready",
            nara_engine::tooling::EditorProjectBrowserStatus::ReadyWithDiagnostics => {
                "Some resources are unavailable"
            }
            nara_engine::tooling::EditorProjectBrowserStatus::Unavailable => {
                "Project resources unavailable"
            }
        };
        page = page
            .child(rows)
            .child(
                view.selected_path()
                    .unwrap_or("No resource selected")
                    .to_owned(),
            )
            .child(format!(
                "{} · {} results{} · {}",
                index_status,
                view.rows().len(),
                if view.rows().len() > 48 {
                    " (refine query for more)"
                } else {
                    ""
                },
                self.status
            ));
        page.into_any_element()
    }
}
