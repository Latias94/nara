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
    sprite: Option<SceneEntityId>,
    selected: bool,
    x: Option<f64>,
}

impl InspectorProjection {
    fn from_snapshot(snapshot: &EditorPanelsSnapshot) -> Self {
        Self {
            sprite: snapshot.sprite.clone(),
            selected: snapshot.sprite.is_some() && snapshot.selected == snapshot.sprite,
            x: snapshot.x,
        }
    }
}

fn field_text(x: Option<f64>) -> String {
    x.map(|value| value.to_string()).unwrap_or_default()
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
    draft_basis: EditorDocumentQueryBasis,
    draft_value: Option<f64>,
    pending_text: Option<String>,
    conflict: bool,
    _subscription: Subscription,
}

impl Inspector {
    pub(super) fn has_conflict(&self) -> bool {
        self.conflict
    }

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
        let draft_value = projection.x;
        let draft = cx.new(|cx| {
            let mut input = DraftInput::new(window, cx);
            input.rebind(field_text(draft_value), window, cx);
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
            draft_basis: basis.clone(),
            draft_value,
            basis,
            pending_text: None,
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
        let snapshot = &model.read(cx).snapshot;
        let projection = InspectorProjection::from_snapshot(snapshot);
        let basis = snapshot.basis.clone();
        let rebound =
            !same_binding(&self.basis, &basis) || self.projection.sprite != projection.sprite;
        let was_conflicted = self.conflict;
        let input = self.draft.read(cx);
        let clean = !was_conflicted
            && !input.is_composing()
            && input.text() == field_text(self.draft_value);
        if rebound || clean {
            if rebound || self.draft_value.map(f64::to_bits) != projection.x.map(f64::to_bits) {
                self.bind_draft(basis.clone(), projection.x, window, cx);
            } else {
                self.draft_basis = basis.clone();
            }
        }
        let changed = self.projection != projection;
        self.projection = projection;
        self.conflict = self.draft_basis != basis;
        self.basis = basis;
        if changed || rebound || was_conflicted != self.conflict {
            cx.notify();
        }
    }

    fn apply(
        &mut self,
        displayed: &InspectorProjection,
        basis: &EditorDocumentQueryBasis,
        cx: &mut Context<Self>,
    ) {
        if self.pending_text.is_some() {
            return;
        }
        if self.draft_basis != self.model.read(cx).snapshot.basis
            || *displayed != self.projection
            || !same_binding(basis, &self.basis)
        {
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
        let Some(entity) = displayed.sprite.clone().filter(|_| displayed.selected) else {
            return;
        };
        self.pending_text = Some(draft.text().to_owned());
        self.commands.borrow_mut().push_back(Command::SetX {
            basis: self.draft_basis.clone(),
            entity,
            value,
            origin: Some(cx.weak_entity()),
        });
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bind_draft(self.basis.clone(), self.projection.x, window, cx);
        cx.notify();
    }

    fn bind_draft(
        &mut self,
        basis: EditorDocumentQueryBasis,
        value: Option<f64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft_basis = basis;
        self.draft_value = value;
        self.conflict = false;
        self.error = None;
        self.draft.update(cx, |input, cx| {
            input.rebind(field_text(value), window, cx);
        });
    }

    pub(super) fn finish_write(
        &mut self,
        before: &EditorDocumentQueryBasis,
        value: f64,
        result: Result<EditorDocumentQueryBasis, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.pending_text.take() else {
            return;
        };
        let input = self.draft.read(cx);
        if self.draft_basis != *before || input.is_composing() || input.text() != text {
            return;
        }
        match result {
            Ok(basis) => self.bind_draft(basis, Some(value), window, cx),
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }
}

impl Render for Inspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let counter = &self.work.inspector_renders[self.index];
        counter.set(counter.get() + 1);
        let projection = self.projection.clone();
        let basis = self.basis.clone();
        let selected_x = projection.x.filter(|_| projection.selected);
        let status = if self.conflict {
            "Source changed; Reload X".into()
        } else {
            self.error.clone().unwrap_or_else(|| {
                selected_x
                    .map(|x| format!("Translation X: {x}"))
                    .unwrap_or_else(|| "Select the Sprite".into())
            })
        };
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
            .child(status)
            .when(selected_x.is_some(), |element| {
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
