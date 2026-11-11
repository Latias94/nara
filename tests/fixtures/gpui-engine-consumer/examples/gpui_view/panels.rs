//! Source-owner projections with independently retained Inspector drafts.

use super::{Command, engine::EditorPanelsSnapshot, text_input::DraftInput};
use nara_engine::{scene::SceneEntityId, tooling::EditorDocumentQueryBasis};
use open_gpui::{
    App, Context, Entity, IntoElement, Render, Subscription, Window, div, prelude::*, px, rgb,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

#[path = "hierarchy.rs"]
mod hierarchy;
pub(super) use hierarchy::Hierarchy;

pub(super) struct EditorModel {
    pub snapshot: EditorPanelsSnapshot,
}

fn same_binding(left: &EditorDocumentQueryBasis, right: &EditorDocumentQueryBasis) -> bool {
    left.document == right.document
        && left.revision.source_id() == right.revision.source_id()
        && left.schema == right.schema
}

impl EditorModel {
    pub fn publish(&mut self, snapshot: EditorPanelsSnapshot, cx: &mut Context<Self>) {
        if self.snapshot != snapshot {
            self.snapshot = snapshot;
            cx.notify();
        }
    }
}

#[derive(Default)]
pub(super) struct PanelWork {
    pub hierarchy_renders: Cell<u64>,
    pub inspector_renders: [Cell<u64>; 2],
    pub projected_notifications: Cell<u64>,
    pub hierarchy_row_callbacks: Cell<u64>,
    pub hierarchy_filter_records: Cell<u64>,
    pub hierarchy_reconciled_records: Cell<u64>,
}

#[derive(Clone, PartialEq)]
struct InspectorProjection {
    selected: Option<SceneEntityId>,
    x: Option<f64>,
}

impl InspectorProjection {
    fn from_snapshot(snapshot: &EditorPanelsSnapshot) -> Self {
        Self {
            selected: snapshot.selected.clone(),
            x: (snapshot.selected.as_ref() == snapshot.sprite.as_ref())
                .then_some(snapshot.x)
                .flatten(),
        }
    }
}

pub(super) struct Inspector {
    model: Entity<EditorModel>,
    projection: InspectorProjection,
    pub draft: Entity<DraftInput>,
    commands: Rc<RefCell<VecDeque<Command>>>,
    work: Rc<PanelWork>,
    index: usize,
    error: Option<String>,
    basis: EditorDocumentQueryBasis,
    conflict: bool,
    _subscription: Subscription,
}

impl Inspector {
    pub fn new(
        index: usize,
        model: Entity<EditorModel>,
        commands: Rc<RefCell<VecDeque<Command>>>,
        work: Rc<PanelWork>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let projection = InspectorProjection::from_snapshot(&model.read(cx).snapshot);
        let basis = model.read(cx).snapshot.basis.clone();
        let draft = cx.new(|cx| {
            let mut input = DraftInput::new(window, cx);
            input.rebind(
                projection.x.map(|x| x.to_string()).unwrap_or_default(),
                window,
                cx,
            );
            input
        });
        let weak = cx.weak_entity();
        let subscription = window.observe(&model, cx, move |model, window, cx| {
            let _ = weak.update(cx, |this, cx| this.synchronize(&model, window, cx));
        });
        Self {
            model,
            projection,
            draft,
            commands,
            work,
            index,
            error: None,
            basis,
            conflict: false,
            _subscription: subscription,
        }
    }

    fn synchronize(
        &mut self,
        model: &Entity<EditorModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.work
            .projected_notifications
            .set(self.work.projected_notifications.get() + 1);
        let projection = InspectorProjection::from_snapshot(&model.read(cx).snapshot);
        let basis = model.read(cx).snapshot.basis.clone();
        let rebound =
            !same_binding(&self.basis, &basis) || self.projection.selected != projection.selected;
        self.basis = basis;
        if rebound || self.projection != projection {
            let input = self.draft.read(cx);
            let clean = !input.is_composing()
                && (input.text() == self.projection.x.map(|x| x.to_string()).unwrap_or_default()
                    || input.text().parse::<f64>().ok() == projection.x);
            if rebound || clean {
                self.draft.update(cx, |input, cx| {
                    input.rebind(
                        projection.x.map(|x| x.to_string()).unwrap_or_default(),
                        window,
                        cx,
                    )
                });
                self.conflict = false;
                self.error = None;
            } else {
                self.conflict = true;
                self.error = Some("Source changed; Reload X".into());
            }
            self.projection = projection;
            cx.notify();
        }
    }

    fn apply(
        &mut self,
        displayed: &InspectorProjection,
        basis: &EditorDocumentQueryBasis,
        cx: &mut Context<Self>,
    ) {
        if self.conflict || *displayed != self.projection || !same_binding(basis, &self.basis) {
            self.error = Some("Source changed; Reload X".into());
            cx.notify();
            return;
        }
        let draft = self.draft.read(cx);
        if draft.is_composing() {
            self.error = Some("Finish composition before Apply".into());
            cx.notify();
            return;
        }
        let Ok(value) = draft.text().parse::<f64>() else {
            self.error = Some("Enter a finite number".into());
            cx.notify();
            return;
        };
        if !value.is_finite() {
            self.error = Some("Enter a finite number".into());
            cx.notify();
            return;
        }
        let Some(entity) = displayed.selected.clone() else {
            return;
        };
        let snapshot = &self.model.read(cx).snapshot;
        let basis = if InspectorProjection::from_snapshot(snapshot) == *displayed
            && same_binding(&snapshot.basis, basis)
        {
            snapshot.basis.clone()
        } else {
            basis.clone()
        };
        self.commands.borrow_mut().push_back(Command::SetX {
            basis,
            entity,
            value,
        });
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft.update(cx, |input, cx| {
            input.rebind(
                self.projection.x.map(|x| x.to_string()).unwrap_or_default(),
                window,
                cx,
            )
        });
        self.conflict = false;
        self.error = None;
        cx.notify();
    }
}

impl Render for Inspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let counter = &self.work.inspector_renders[self.index];
        counter.set(counter.get() + 1);
        let projection = self.projection.clone();
        let basis = self.model.read(cx).snapshot.basis.clone();
        div()
            .size_full()
            .bg(rgb(0x243040))
            .rounded_md()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .line_height(px(20.))
            .child(format!("Inspector {}", self.index + 1))
            .child(self.error.clone().unwrap_or_else(|| {
                self.projection
                    .x
                    .map(|x| format!("Translation X: {x}"))
                    .unwrap_or_else(|| "Select the Sprite".into())
            }))
            .when(self.projection.x.is_some(), |element| {
                element
                    .child(div().h(px(38.)).child(self.draft.clone()))
                    .child(
                        div()
                            .id("apply-x")
                            .h(px(30.))
                            .bg(rgb(0x315878))
                            .cursor_pointer()
                            .child("Apply X")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.apply(&projection, &basis, cx)
                            })),
                    )
            })
            .child(
                div()
                    .id("reload-x")
                    .h(px(24.))
                    .cursor_pointer()
                    .child("Reload X")
                    .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
            )
    }
}

pub(super) fn create_inspectors(
    model: &Entity<EditorModel>,
    commands: &Rc<RefCell<VecDeque<Command>>>,
    work: &Rc<PanelWork>,
    window: &mut Window,
    cx: &mut App,
) -> [Entity<Inspector>; 2] {
    std::array::from_fn(|index| {
        cx.new(|cx| {
            Inspector::new(
                index,
                model.clone(),
                commands.clone(),
                work.clone(),
                window,
                cx,
            )
        })
    })
}
