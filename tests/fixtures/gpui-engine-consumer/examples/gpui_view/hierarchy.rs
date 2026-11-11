//! Paged source declarations rendered by the framework's existing variable-height list.

use super::super::{Command, text_input::DraftInput};
use super::{EditorModel, PanelWork, same_binding};
use nara_engine::{scene::SceneEntityId, tooling::EditorDocumentQueryBasis};
use open_gpui::{
    Bounds, Context, Entity, IntoElement, ListAlignment, ListOffset, ListState, Pixels, Render,
    Subscription, Window, div, list, prelude::*, px, rgb,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

pub(crate) struct Hierarchy {
    model: Entity<EditorModel>,
    entities: Rc<[SceneEntityId]>,
    indices: Option<Rc<[usize]>>,
    selected: Option<SceneEntityId>,
    basis: EditorDocumentQueryBasis,
    state: ListState,
    pub filter_input: Entity<DraftInput>,
    filter: String,
    details: bool,
    commands: Rc<RefCell<VecDeque<Command>>>,
    work: Rc<PanelWork>,
    _subscription: Subscription,
}

fn row_at<'a>(
    entities: &'a [SceneEntityId],
    indices: Option<&[usize]>,
    index: usize,
) -> Option<&'a SceneEntityId> {
    entities.get(match indices {
        Some(indices) => *indices.get(index)?,
        None => index,
    })
}

impl Hierarchy {
    pub(crate) fn new(
        model: Entity<EditorModel>,
        commands: Rc<RefCell<VecDeque<Command>>>,
        work: Rc<PanelWork>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = model.read(cx).snapshot.clone();
        let filter_input = cx.new(|cx| DraftInput::new(window, cx));
        let weak = cx.weak_entity();
        let subscription = window.observe(&model, cx, move |model, window, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.work
                    .projected_notifications
                    .set(this.work.projected_notifications.get() + 1);
                let snapshot = model.read(cx).snapshot.clone();
                let rebound = !same_binding(&this.basis, &snapshot.basis);
                this.basis = snapshot.basis;
                if rebound || this.entities != snapshot.entities {
                    this.replace_rows(snapshot.entities, rebound);
                    if rebound {
                        this.filter_input
                            .update(cx, |input, cx| input.rebind(String::new(), window, cx));
                    }
                    cx.notify();
                }
                if this.selected != snapshot.selected {
                    this.selected = snapshot.selected;
                    cx.notify();
                }
            });
        });
        Self {
            model,
            state: ListState::new(snapshot.entities.len(), ListAlignment::Top, px(45.))
                .with_uniform_item_height(px(30.)),
            entities: snapshot.entities,
            indices: None,
            selected: snapshot.selected,
            basis: snapshot.basis,
            filter_input,
            filter: String::new(),
            details: false,
            commands,
            work,
            _subscription: subscription,
        }
    }

    pub(crate) fn row_count(&self) -> usize {
        self.indices
            .as_ref()
            .map_or(self.entities.len(), |indices| indices.len())
    }

    pub(crate) fn anchor(&self) -> Option<(SceneEntityId, Pixels)> {
        let anchor = self.state.logical_scroll_top();
        row_at(&self.entities, self.indices.as_deref(), anchor.item_ix)
            .cloned()
            .map(|entity| (entity, anchor.offset_in_item))
    }

    pub(crate) fn bounds_for(&self, entity: &SceneEntityId) -> Option<Bounds<Pixels>> {
        let index = (0..self.row_count()).find(|index| {
            row_at(&self.entities, self.indices.as_deref(), *index) == Some(entity)
        })?;
        self.state.bounds_for_item(index)
    }

    pub(crate) fn viewport(&self) -> Bounds<Pixels> {
        self.state.viewport_bounds()
    }

    pub(crate) fn reveal(
        &mut self,
        entity: &SceneEntityId,
        offset: Pixels,
        cx: &mut Context<Self>,
    ) {
        if let Some(item_ix) = (0..self.row_count())
            .find(|index| row_at(&self.entities, self.indices.as_deref(), *index) == Some(entity))
        {
            self.state.scroll_to(ListOffset {
                item_ix,
                offset_in_item: offset,
            });
            cx.notify();
        }
    }

    fn replace_rows(&mut self, entities: Rc<[SceneEntityId]>, rebound: bool) {
        let old_entities = self.entities.clone();
        let old_indices = self.indices.clone();
        let old_count = self.row_count();
        let anchor = (!rebound).then(|| self.anchor()).flatten();
        if rebound {
            self.filter.clear();
        }
        self.entities = entities;
        self.indices = if self.filter.is_empty() {
            None
        } else {
            Some(
                self.entities
                    .iter()
                    .enumerate()
                    .filter_map(|(index, entity)| {
                        self.work
                            .hierarchy_filter_records
                            .set(self.work.hierarchy_filter_records.get() + 1);
                        entity
                            .as_str()
                            .to_lowercase()
                            .contains(&self.filter)
                            .then_some(index)
                    })
                    .collect::<Vec<_>>()
                    .into(),
            )
        };
        let new_count = self.row_count();
        if rebound {
            self.state.reset_with_uniform_height(new_count, px(30.));
            return;
        }
        let mut prefix = 0;
        while prefix < old_count.min(new_count) {
            self.work
                .hierarchy_reconciled_records
                .set(self.work.hierarchy_reconciled_records.get() + 1);
            if row_at(&old_entities, old_indices.as_deref(), prefix)
                != row_at(&self.entities, self.indices.as_deref(), prefix)
            {
                break;
            }
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < old_count.min(new_count) - prefix {
            self.work
                .hierarchy_reconciled_records
                .set(self.work.hierarchy_reconciled_records.get() + 1);
            if row_at(
                &old_entities,
                old_indices.as_deref(),
                old_count - suffix - 1,
            ) != row_at(
                &self.entities,
                self.indices.as_deref(),
                new_count - suffix - 1,
            ) {
                break;
            }
            suffix += 1;
        }
        if prefix + suffix < old_count || prefix + suffix < new_count {
            self.state
                .splice(prefix..old_count - suffix, new_count - prefix - suffix);
        }
        if let Some((entity, offset)) = anchor {
            let destination = (0..new_count).find(|index| {
                self.work
                    .hierarchy_reconciled_records
                    .set(self.work.hierarchy_reconciled_records.get() + 1);
                row_at(&self.entities, self.indices.as_deref(), *index) == Some(&entity)
            });
            self.state.scroll_to(ListOffset {
                item_ix: destination.unwrap_or(0),
                offset_in_item: if destination.is_some() {
                    offset
                } else {
                    px(0.)
                },
            });
        }
    }

    fn apply_filter(&mut self, cx: &mut Context<Self>) {
        let input = self.filter_input.read(cx);
        if !input.is_composing() {
            self.filter = input.text().to_lowercase();
            self.replace_rows(self.entities.clone(), false);
            cx.notify();
        }
    }

    pub(crate) fn toggle_details(&mut self, cx: &mut Context<Self>) {
        self.details = !self.details;
        self.state.remeasure();
        cx.notify();
    }
}

impl Render for Hierarchy {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.work
            .hierarchy_renders
            .set(self.work.hierarchy_renders.get() + 1);
        let entities = self.entities.clone();
        let indices = self.indices.clone();
        let selected = self.selected.clone();
        let basis = self.basis.clone();
        let commands = self.commands.clone();
        let model = self.model.clone();
        let work = self.work.clone();
        let details = self.details;
        let sprite = self.model.read(cx).snapshot.sprite.clone();
        let rows = list(self.state.clone(), move |index, _, _| {
            work.hierarchy_row_callbacks
                .set(work.hierarchy_row_callbacks.get() + 1);
            let entity = row_at(&entities, indices.as_deref(), index)
                .expect("the current list index belongs to this source projection")
                .clone();
            let label = if sprite.as_ref() == Some(&entity) {
                "Sprite".to_owned()
            } else {
                entity.to_string()
            };
            let expanded = details || entity.as_str().ends_with('0');
            let current_entities = entities.clone();
            let displayed_selection = selected.clone();
            let basis = basis.clone();
            let commands = commands.clone();
            let model = model.clone();
            div()
                .id(format!(
                    "source-row:{:?}:{entity}",
                    basis.revision.source_id()
                ))
                .h(px(if expanded { 52. } else { 30. }))
                .px_2()
                .py_1()
                .flex()
                .flex_col()
                .line_height(px(20.))
                .bg(if selected.as_ref() == Some(&entity) {
                    rgb(0x315878)
                } else {
                    rgb(0x243040)
                })
                .cursor_pointer()
                .child(label)
                .when(expanded, |row| row.child("Source declaration"))
                .on_click(move |_, _, cx| {
                    let current = &model.read(cx).snapshot;
                    let basis = if current.entities == current_entities
                        && current.selected == displayed_selection
                        && same_binding(&current.basis, &basis)
                    {
                        current.basis.clone()
                    } else {
                        basis.clone()
                    };
                    commands.borrow_mut().push_back(Command::Select {
                        basis,
                        entity: entity.clone(),
                    });
                })
                .into_any_element()
        })
        .w_full()
        .h(px(58.))
        .flex_shrink_0();
        div()
            .size_full()
            .bg(rgb(0x243040))
            .rounded_md()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(24.))
                    .flex_shrink_0()
                    .flex()
                    .justify_between()
                    .child(format!("Hierarchy · {} source rows", self.row_count()))
                    .child(
                        div()
                            .id("row-details")
                            .cursor_pointer()
                            .child("Details")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_details(cx))),
                    ),
            )
            .child(
                div()
                    .h(px(48.))
                    .flex_shrink_0()
                    .flex()
                    .child(
                        div()
                            .w(px(250.))
                            .h(px(48.))
                            .child(self.filter_input.clone()),
                    )
                    .child(
                        div()
                            .id("filter-source")
                            .w(px(70.))
                            .cursor_pointer()
                            .child("Filter")
                            .on_click(cx.listener(|this, _, _, cx| this.apply_filter(cx))),
                    ),
            )
            .child(rows)
    }
}
