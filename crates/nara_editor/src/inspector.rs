mod color;
mod numeric;
mod property;
mod target;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    rc::Rc,
};

use nara_editor_core::{InspectorModel, value_draft::RootDraft};
use nara_reflect::{ComponentSchema, ComponentSchemaVersion, ComponentTypeId};
use open_gpui::{
    App, Context, Div, Entity, EntityId, FontWeight, IntoElement, Render, Rgba, StyleRefinement,
    Subscription, Window, div, prelude::*, px, rgb,
};
use open_gpui_base::{Button, Menu, MenuEvent, MenuItem, Select, TextInput, TextInputEvent};

use crate::{theme, workspace::Workspace};

use self::{
    property::{Property, PropertyEditor},
    target::{AddComponent, CHANGED, ComponentRemoval, FieldKey, Selection},
};

struct NameEdit {
    input: Entity<TextInput>,
    _subscription: Subscription,
    current: String,
    initial_text: String,
    expected: Option<String>,
    error: Option<String>,
}

impl NameEdit {
    fn draft(&self, cx: &App) -> Option<Draft> {
        self.expected.as_ref()?;
        let value = self.input.read(cx).value();
        (value != self.initial_text).then(|| Draft::Text(value.to_owned()))
    }

    fn reset_input(&mut self, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.set_value(self.current.clone(), cx));
        self.initial_text = self.input.read(cx).value().to_owned();
    }
}

#[derive(PartialEq)]
enum Draft {
    Text(String),
    Structure(Rc<RootDraft>),
    Color(Rgba),
}

#[derive(Default, PartialEq)]
pub(crate) struct InspectorDrafts(HashMap<EntityId, Draft>);

impl InspectorDrafts {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

struct ComponentSection {
    id: ComponentTypeId,
    title: String,
    opaque: Option<ComponentSchemaVersion>,
    fields: Vec<(FieldKey, Entity<PropertyEditor>)>,
    removal: Option<Rc<ComponentRemoval>>,
}

struct AddMenu {
    view: Entity<Menu<ComponentTypeId>>,
    _subscription: Subscription,
    options: Rc<Vec<AddComponent>>,
}

pub(crate) struct InspectorPanel {
    workspace: Entity<Workspace>,
    _workspace_subscription: Subscription,
    selection: Rc<Selection>,
    revision: Option<u64>,
    schemas: Vec<ComponentSchema>,
    name: Option<NameEdit>,
    sections: Vec<ComponentSection>,
    partial: Vec<(String, Rc<ComponentRemoval>)>,
    collapsed: HashSet<ComponentTypeId>,
    add_menu: Option<AddMenu>,
    suspended_names: HashMap<Rc<Selection>, NameEdit>,
    suspended_fields: HashMap<(Rc<Selection>, FieldKey), Entity<PropertyEditor>>,
}

impl InspectorPanel {
    #[cfg(all(test, windows))]
    pub(crate) fn name_input(&self) -> Option<&Entity<TextInput>> {
        self.name.as_ref().map(|edit| &edit.input)
    }

    pub(crate) fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        let selection = Rc::new(Selection::new(&workspace.read(cx).session));
        let subscription = cx.observe(&workspace, |panel, _, cx| {
            if panel.refresh(cx) {
                cx.notify();
            }
        });
        let mut panel = Self {
            workspace,
            _workspace_subscription: subscription,
            selection,
            revision: None,
            schemas: Vec::new(),
            name: None,
            sections: Vec::new(),
            partial: Vec::new(),
            collapsed: HashSet::new(),
            add_menu: None,
            suspended_names: HashMap::new(),
            suspended_fields: HashMap::new(),
        };
        panel.refresh(cx);
        panel
    }

    pub(crate) fn drafts(&self, cx: &App) -> InspectorDrafts {
        let names = self
            .name
            .iter()
            .chain(self.suspended_names.values())
            .filter_map(|edit| Some((edit.input.entity_id(), edit.draft(cx)?)));
        let fields = self
            .sections
            .iter()
            .flat_map(|section| section.fields.iter().map(|(_, field)| field))
            .chain(self.suspended_fields.values())
            .filter_map(|field| Some((field.entity_id(), field.read(cx).draft(cx)?)));
        InspectorDrafts(names.chain(fields).collect())
    }

    fn refresh(&mut self, cx: &mut Context<Self>) -> bool {
        let workspace = self.workspace.read(cx);
        let session = &workspace.session;
        let revision = workspace.revision;
        let rebound = !self.selection.matches(session);
        let schema_changed = !session.registry().schemas().eq(&self.schemas);
        if !rebound && !schema_changed && self.revision == Some(revision) {
            return false;
        }
        if schema_changed {
            self.schemas = session.registry().schemas().cloned().collect();
        }
        let selection = if rebound {
            Rc::new(Selection::new(session))
        } else {
            self.selection.clone()
        };
        let model = InspectorModel::for_session(session);
        let rows = selection.rows(session).unwrap_or_default();
        let name = match rows.as_slice() {
            [row] => Some(row.name.clone()),
            _ => None,
        };
        let label = |id: &ComponentTypeId| {
            session
                .registry()
                .schema(id)
                .and_then(|schema| schema.aliases().first().cloned())
                .unwrap_or_else(|| id.as_str().to_owned())
        };
        let mut placements = BTreeMap::<_, Vec<_>>::new();
        for row in &rows {
            for (id, record) in &row.components {
                placements
                    .entry(id)
                    .or_default()
                    .push((row.id, record.version));
            }
        }
        let projected: Vec<_> = model
            .components
            .iter()
            .map(|component| {
                let properties = Property::for_component(&selection, &rows, component);
                let opaque = component.opaque.as_ref().map(|_| component.version);
                let removal = placements
                    .remove(&component.id)
                    .map(|present| Rc::new(ComponentRemoval::new(component.id.clone(), present)));
                (
                    component.id.clone(),
                    label(&component.id),
                    opaque,
                    properties,
                    removal,
                )
            })
            .collect();
        let partial = placements
            .into_iter()
            .map(|(id, present)| {
                let title = format!(
                    "{} · {} of {} selected",
                    label(id),
                    present.len(),
                    rows.len()
                );
                (title, Rc::new(ComponentRemoval::new(id.clone(), present)))
            })
            .collect();
        let options = AddComponent::options(session, &rows);
        let previous = std::mem::replace(&mut self.selection, selection);
        self.revision = Some(revision);
        self.partial = partial;

        let mut existing: HashMap<_, _> = self
            .sections
            .drain(..)
            .flat_map(|section| section.fields)
            .collect();
        if rebound {
            if let Some(name) = self.name.take().filter(|name| name.draft(cx).is_some()) {
                self.suspended_names.insert(previous.clone(), name);
            }
            for (key, view) in existing.drain() {
                if view.update(cx, PropertyEditor::suspend) {
                    self.suspended_fields.insert((previous.clone(), key), view);
                }
            }
            self.name = self.suspended_names.remove(&self.selection);
            self.add_menu = None;
        }
        self.refresh_name(name, cx);
        self.sections = projected
            .into_iter()
            .map(|(id, title, opaque, properties, removal)| {
                let fields = properties
                    .into_iter()
                    .map(|property| {
                        let key = property.target.key.clone();
                        let identity = (self.selection.clone(), key.clone());
                        let view = existing
                            .remove(&key)
                            .or_else(|| self.suspended_fields.remove(&identity));
                        let view = match view {
                            Some(view) if view.read(cx).can_refresh(&property) => {
                                view.update(cx, |editor, cx| editor.refresh(property, cx));
                                view
                            }
                            previous => {
                                if let Some(view) = previous {
                                    if view.update(cx, PropertyEditor::suspend) {
                                        self.suspended_fields.insert(identity, view);
                                    }
                                }
                                cx.new(|cx| {
                                    PropertyEditor::new(self.workspace.clone(), property, cx)
                                })
                            }
                        };
                        (key, view)
                    })
                    .collect();
                ComponentSection {
                    id,
                    title,
                    opaque,
                    fields,
                    removal,
                }
            })
            .collect();
        for (key, view) in existing {
            if view.update(cx, PropertyEditor::suspend) {
                self.suspended_fields
                    .insert((self.selection.clone(), key), view);
            }
        }
        self.refresh_add_menu(options, cx);
        true
    }

    fn refresh_name(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        let Some(name) = name else {
            self.name = None;
            return;
        };
        if let Some(edit) = &mut self.name {
            if edit.current != name {
                edit.current = name;
                if edit.expected.is_none() {
                    edit.reset_input(cx);
                }
            }
            return;
        }
        let input = cx.new(|cx| native_text(name.clone(), "Entity name", "", cx));
        let subscription = cx.subscribe(&input, Self::rename_event);
        self.name = Some(NameEdit {
            initial_text: input.read(cx).value().to_owned(),
            input,
            _subscription: subscription,
            current: name,
            expected: None,
            error: None,
        });
    }

    fn rename_event(
        &mut self,
        input: Entity<TextInput>,
        event: &TextInputEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self
            .name
            .as_mut()
            .filter(|edit| edit.input.entity_id() == input.entity_id())
        else {
            return;
        };
        match event {
            TextInputEvent::Change(_) => {
                edit.expected.get_or_insert_with(|| edit.current.clone());
                edit.error = None;
            }
            TextInputEvent::Cancel => {
                edit.expected = None;
                edit.error = None;
                edit.reset_input(cx);
            }
            TextInputEvent::Submit(_) | TextInputEvent::Blur => {
                let Some(expected) = edit.expected.as_ref() else {
                    return;
                };
                let name = input.read(cx).value().to_owned();
                edit.error = self
                    .workspace
                    .update(cx, |workspace, cx| {
                        let Some(command) =
                            self.selection
                                .rename_command(&workspace.session, expected, name)?
                        else {
                            return Ok(());
                        };
                        if workspace.session.gesture_active() {
                            return Err("Finish the active gesture before renaming.");
                        }
                        workspace
                            .apply(command, cx)
                            .then_some(())
                            .ok_or("The name was rejected. See the Console for details.")
                    })
                    .err()
                    .map(str::to_owned);
                if edit.error.is_none() {
                    edit.expected = None;
                    edit.initial_text = input.read(cx).value().to_owned();
                }
            }
        }
        input.update(cx, |input, cx| input.set_invalid(edit.error.is_some(), cx));
        cx.notify();
    }

    fn refresh_add_menu(&mut self, options: Vec<AddComponent>, cx: &mut Context<Self>) {
        if options.is_empty() {
            self.add_menu = None;
            return;
        }
        if self
            .add_menu
            .as_ref()
            .is_some_and(|menu| *menu.options == options)
        {
            return;
        }
        let view = cx.new(|cx| {
            let items = options
                .iter()
                .map(|option| MenuItem::new(option.id.clone(), option.label.clone()));
            let mut menu = Select::menu("Add Component…", items, cx);
            menu.set_trigger_style(theme::input(), cx);
            menu.set_popup_style(popup_style(), cx);
            menu
        });
        let options = Rc::new(options);
        let choices = options.clone();
        let selection = self.selection.clone();
        let subscription = cx.subscribe(&view, move |panel, _, event, cx| {
            let MenuEvent::Selected(id) = event else {
                return;
            };
            let Some(option) = choices.iter().find(|option| &option.id == id) else {
                return;
            };
            panel.workspace.update(cx, |workspace, cx| {
                if workspace.session.gesture_active() {
                    workspace.warn("Finish the active gesture before adding a component.", cx);
                } else if let Some(command) = option.command(&workspace.session, &selection) {
                    workspace.apply(command, cx);
                } else {
                    workspace.warn(
                        "The selection or component schema changed. Open Add Component again.",
                        cx,
                    );
                }
            });
        });
        self.add_menu = Some(AddMenu {
            view,
            _subscription: subscription,
            options,
        });
    }

    fn remove_button(&self, removal: &Rc<ComponentRemoval>, cx: &Context<Self>) -> Button {
        let removal = removal.clone();
        let selection = self.selection.clone();
        theme::button("remove-component", "Remove")
            .flex_shrink_0()
            .text_color(rgb(theme::ERROR))
            .on_click(cx.listener(move |panel, _, _, cx| {
                panel.workspace.update(cx, |workspace, cx| {
                    if workspace.session.gesture_active() {
                        workspace
                            .warn("Finish the active gesture before removing a component.", cx);
                    } else if let Some(command) = removal.command(&workspace.session, &selection) {
                        workspace.apply(command, cx);
                    } else {
                        workspace.warn(
                            "The selection or component changed. Review the Inspector.",
                            cx,
                        );
                    }
                });
            }))
    }

    fn render_section(&self, section: &ComponentSection, cx: &Context<Self>) -> impl IntoElement {
        let collapsed = self.collapsed.contains(&section.id);
        let id = section.id.clone();
        let arrow = if collapsed { "▸" } else { "▾" };
        let heading = format!("{arrow}  {}", section.title);
        let header = theme::button("collapse-component", &heading)
            .flex_1()
            .min_w_0()
            .text_left()
            .font_weight(FontWeight::SEMIBOLD)
            .bg(rgb(theme::RAISED))
            .expanded(!collapsed)
            .on_click(cx.listener(move |panel, _, _, cx| {
                if !panel.collapsed.remove(&id) {
                    panel.collapsed.insert(id.clone());
                    if let Some(section) = panel.sections.iter().find(|section| section.id == id) {
                        for (_, field) in &section.fields {
                            field.update(cx, PropertyEditor::cancel_scrub);
                        }
                    }
                }
                cx.notify();
            }));
        let mut container = div()
            .id(section.id.as_str().to_owned())
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .border_1()
            .border_color(rgb(theme::BORDER))
            .rounded(px(4.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .bg(rgb(theme::RAISED))
                    .child(header)
                    .children(
                        section
                            .removal
                            .as_ref()
                            .map(|removal| self.remove_button(removal, cx)),
                    ),
            );
        if !collapsed {
            if let Some(version) = section.opaque {
                container = container.child(note(format!(
                    "Read only · Schema unavailable for version {}",
                    version.get()
                )));
            } else if section.fields.is_empty() {
                container = container.child(note("No inspectable fields"));
            } else {
                container = container.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .p_1()
                        .children(section.fields.iter().map(|(_, view)| view.clone())),
                );
            }
        }
        container
    }
}

impl Render for InspectorPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div()
            .id("inspector-content")
            .min_h_0()
            .min_w_0()
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2()
            .p_2();
        let count = self.selection.targets.len();
        if count == 0 {
            content = content.child(note("Select an entity to inspect it."));
        } else if let Some(edit) = &self.name {
            content = content.child(field_row("Name", edit.input.clone()));
            let conflict = edit
                .expected
                .as_ref()
                .is_some_and(|expected| *expected != edit.current);
            if let Some(error) = if conflict {
                Some(CHANGED)
            } else {
                edit.error.as_deref()
            } {
                content = content.child(note(error.to_owned()).text_color(rgb(theme::ERROR)));
            }
        } else {
            content = content.child(note(format!("{count} entities selected")));
        }
        content = content
            .children(
                self.sections
                    .iter()
                    .map(|section| self.render_section(section, cx)),
            )
            .children(self.partial.iter().map(|(title, removal)| {
                div()
                    .id(removal.id.as_str().to_owned())
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(note(title.clone()).flex_1().min_w_0())
                    .child(self.remove_button(removal, cx))
            }))
            .when(
                !self.suspended_fields.is_empty() || !self.suspended_names.is_empty(),
                |content| {
                    content.child(note(
                        "Unapplied edits are held for their original selection.",
                    ))
                },
            );
        let toolbar = theme::toolbar()
            .children(self.add_menu.as_ref().map(|menu| menu.view.clone()))
            .when(self.add_menu.is_none(), |toolbar| {
                toolbar.child(theme::button("add-component", "Add Component…").disabled(true))
            });
        theme::panel()
            .child(content)
            .children((count > 0).then_some(toolbar))
    }
}

fn note(message: impl Into<open_gpui::SharedString>) -> Div {
    div()
        .px_2()
        .py_1()
        .text_color(rgb(theme::MUTED))
        .child(message.into())
}

fn field_row(label: &str, control: impl IntoElement) -> Div {
    div()
        .w_full()
        .min_w_0()
        .min_h(px(28.))
        .flex()
        .items_center()
        .gap_2()
        .px_1()
        .child(
            div()
                .w(open_gpui::relative(0.4))
                .max_w(px(116.))
                .flex_shrink_0()
                .text_color(rgb(theme::MUTED))
                .child(label.to_owned()),
        )
        .child(div().flex_1().min_w_0().child(control))
}

fn native_text(
    value: String,
    label: &str,
    placeholder: &str,
    cx: &mut Context<TextInput>,
) -> TextInput {
    let mut input = TextInput::new(value, cx);
    input.set_style(theme::input().w_full(), cx);
    input.set_label(label.to_owned(), cx);
    input.set_placeholder(placeholder.to_owned(), cx);
    input
}

fn popup_style() -> StyleRefinement {
    StyleRefinement::default()
        .bg(rgb(theme::RAISED))
        .text_color(rgb(theme::TEXT))
        .border_1()
        .border_color(rgb(theme::BORDER))
        .rounded(px(4.))
        .px_2()
        .py_1()
}
