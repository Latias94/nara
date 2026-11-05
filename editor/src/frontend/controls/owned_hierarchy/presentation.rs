use std::{
    borrow::Cow,
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

use nara::tooling::SceneInspectorEntityRow;
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiDimension, UiEdges, UiLayer, UiLocalKey, UiNode,
    UiOverflow, UiRoot, UiRootStatus, UiStyle, UiTextOverflow, UiVisual,
};
use nara_ui_widgets::{UiAppearanceError, UiAppearancePalette, UiButtonVariant, UiControlSize};

use super::*;

const MAX_PRESENTATION_BYTES: usize = 128;
// Leave the rest of the shared frame's glyph allowance for chrome and the local filter.
const ROW_PRESENTATION_BYTES: usize = 2048;

#[derive(Clone, Copy)]
pub(super) struct Metrics {
    appearance: UiAppearance,
    body: f32,
    caption: f32,
    title: f32,
    action: f32,
    field: f32,
    source_row: f32,
}

impl Metrics {
    fn new(appearance: UiAppearance, engine: &mut TextEngine) -> Result<Self, UiAppearanceError> {
        let mut measure = |role| {
            let layout = engine.layout(&nara_text::TextLayoutRequest::from_typography(
                "Ag",
                appearance.typography(role)?,
            ))?;
            Ok::<_, UiAppearanceError>(line_box(&layout))
        };
        let body = measure(UiTextRole::Body)?;
        let caption = measure(UiTextRole::Caption)?;
        let title = measure(UiTextRole::Title)?;
        let action = appearance.control_metrics(UiControlSize::Small)?;
        let field = appearance.control_metrics(UiControlSize::Regular)?;
        let action = action
            .height()
            .max(body + action.padding().top() + action.padding().bottom());
        let field = field
            .height()
            .max(body + field.padding().top() + field.padding().bottom());
        Ok(Self {
            appearance,
            body,
            caption,
            title,
            action,
            field,
            source_row: action + 2.0 * caption + 2.0 * appearance.gap(),
        })
    }
}

impl OwnedHierarchy {
    pub(in crate::frontend::controls) fn compile(
        &mut self,
        model: &EditorFrameModel,
        environment: UiEnvironment,
        theme: UiTheme,
        read_only: bool,
    ) -> Result<Arc<UiFrame>, OwnedHierarchyError> {
        self.work.compile_calls = self.work.compile_calls.saturating_add(1);
        if !self.explicit_appearance
            && self
                .frames
                .last()
                .is_none_or(|previous| previous.theme != theme)
        {
            let appearance = reference_appearance(theme)?;
            if self.appearance != appearance {
                self.appearance = appearance;
                self.invalidate_actions();
            }
        }
        let painted_clean = !self.paint_dirty;
        self.paint_dirty = true;
        let appearance = self.appearance;
        let inset = appearance.panel_inset();
        let gap = appearance.gap();
        let width = environment.logical_size().width() - 2.0 * inset;
        let height = environment.logical_size().height();
        if width <= 1.0 || height <= 0.0 {
            return Err(OwnedHierarchyError::InvalidViewport);
        }
        if painted_clean
            && let Some(previous) = self.frames.last()
            && previous.model_revision == model.revision()
            && previous.read_only == read_only
            && previous.frame.environment() == environment
            && previous.theme == theme
            && previous.appearance == appearance
            && Arc::ptr_eq(&previous.observation, &self.observation)
            && previous.observation.load(Ordering::Relaxed)
        {
            self.work.reused_frames = self.work.reused_frames.saturating_add(1);
            self.projection_work.early_reused_frames =
                self.projection_work.early_reused_frames.saturating_add(1);
            self.paint_dirty = false;
            return Ok(Arc::clone(&previous.frame));
        }
        let metrics = match self.metrics {
            Some(metrics)
                if metrics.appearance.density() == appearance.density()
                    && metrics.appearance.text_scale() == appearance.text_scale() =>
            {
                metrics
            }
            _ => Metrics::new(appearance, &mut self.engine)?,
        };
        self.metrics = Some(metrics);
        if self
            .projection
            .as_ref()
            .is_none_or(|projection| !projection.matches(model, read_only))
        {
            self.projection = Some(SourceProjection::capture(
                model,
                read_only,
                &mut self.projection_work,
            ));
        }
        let projection = self
            .projection
            .as_ref()
            .expect("current projection captured");
        let basis = projection.basis.clone();
        let selected_exists = projection.selected_index.is_some();
        let scope = basis.as_ref().map(HierarchyBasis::scope);
        if self.scope != scope {
            self.filter.blur(&mut self.engine)?;
            self.filter = UiTextField::with_typography(
                self.root.clone(),
                "Filter entities",
                "",
                appearance.typography(UiTextRole::Body)?,
                &mut self.engine,
            )?;
            self.scope = scope;
            self.source_scroll = 0.0;
            self.reveal_selection = false;
            self.input_error = None;
        }
        if self.basis != basis {
            if let Some(choices) = self.choices.take() {
                self.error.get_or_insert_with(|| format!(
                    "Choices for {} were retired because the source, selection or permissions changed",
                    choices.basis.selected.as_ref().map_or("the original entity", SceneEntityId::as_str),
                ));
            }
            self.choice_scroll = 0.0;
            self.identities.clear();
            self.controls.clear();
            self.invalidate_actions();
            self.basis = basis.clone();
        }

        let mut nodes = Vec::new();
        let mut buttons = HashMap::new();
        self.filter
            .reconfigure(&mut self.engine, appearance.typography(UiTextRole::Body)?)?;
        let mut field_appearance = appearance.field(
            self.filter.input_token().is_some(),
            self.filter.enabled(),
            self.input_error.is_some(),
        )?;
        let padding = field_appearance.padding;
        if width <= padding.left() + padding.right() {
            let inline = (width - 1.0) * 0.5;
            field_appearance.padding =
                UiEdges::new(inline, inline, padding.top(), padding.bottom())?;
        }
        let field_height = metrics
            .field
            .max(line_box(self.filter.editor().layout()) + padding.top() + padding.bottom());
        self.filter.set_appearance(field_appearance);
        let action_height = metrics.action;
        let top = if height < 4.0 * action_height + 2.0 * inset {
            2.0
        } else {
            inset
        };
        let half = ((width - gap) / 2.0).max(1.0);
        let busy = self.queued || self.pending.is_some();
        let editing = basis.as_ref().is_some_and(|basis| basis.editing) && !busy;
        let selected = basis.as_ref().and_then(|basis| basis.selected.as_ref());
        let mut source_limit = 0.0;
        let mut choice_limit = 0.0;
        let show_filter = self.removal.is_none()
            && self.choices.is_none()
            && (self.filter.input_token().is_some()
                || height
                    >= 2.0 * top + field_height + 2.0 * action_height + 3.0 * gap + metrics.body);
        if !show_filter {
            self.filter.blur(&mut self.engine)?;
        }
        if let Some(removal) = self.removal.clone() {
            let entity = removal
                .selected
                .as_ref()
                .map_or("the original entity", SceneEntityId::as_str);
            let unchanged = basis.as_ref() == Some(&removal);
            nodes.push(self.button(
                Control::ConfirmRemoval,
                "Confirm removal",
                editing && unchanged && removal.removal_reason.is_none(),
                [inset, top, half, action_height],
                false,
                &mut buttons,
            )?);
            nodes.push(self.button(
                Control::CancelRemoval,
                "Cancel removal",
                true,
                [inset + half + gap, top, half, action_height],
                false,
                &mut buttons,
            )?);
            let y = top + action_height + gap;
            let viewport_height = (height - y - top).max(0.0);
            let mut messages = vec![
                ("removal-question", format!("Remove the source-local branch rooted at {entity}?")),
                ("removal-warning", if unchanged {
                    "Removal is refused if the source-local hierarchy cannot be resolved"
                } else {
                    "The original selection or source changed. Cancel this confirmation before retrying."
                }.to_owned()),
            ];
            if let Some(error) = &self.error {
                messages.push(("error", error.clone()));
            }
            let row_height = 2.0 * metrics.body + gap;
            choice_limit = (messages.len() as f32 * row_height - viewport_height).max(0.0);
            self.choice_scroll = self.choice_scroll.clamp(0.0, choice_limit);
            let mut children = Vec::new();
            for index in visible_range(
                messages.len(),
                self.choice_scroll,
                viewport_height,
                row_height,
            ) {
                let (identity, message) = &messages[index];
                children.push(wrapped_label(
                    appearance,
                    identity,
                    message,
                    [
                        0.0,
                        index as f32 * row_height - self.choice_scroll,
                        width,
                        row_height - gap,
                    ],
                )?);
            }
            if viewport_height > 0.0 {
                nodes.push(viewport(inset, y, width, viewport_height, children));
            }
        } else if let Some(choices) = &self.choices {
            let menu_kind = choices.kind;
            let menu_basis = choices.basis.clone();
            let scene = model
                .workspace()
                .active_scene
                .as_ref()
                .ok_or(OwnedHierarchyError::ChangedProjection)?;
            let count = scene.editor.inspector.entities.len() - usize::from(selected_exists);
            let count = match menu_kind {
                MenuKind::Parent => count + 1,
                MenuKind::Order => count * 2 + 2,
            };
            let side_by_side = height < 2.0 * top + 2.0 * action_height + gap;
            let close_width = if side_by_side { half } else { width };
            nodes.push(self.button(
                Control::CloseChoices,
                "Close choices",
                true,
                [inset, top, close_width, action_height],
                false,
                &mut buttons,
            )?);
            let viewport_x = if side_by_side {
                inset + half + gap
            } else {
                inset
            };
            let viewport_width = if side_by_side { half } else { width };
            let mut y = if side_by_side {
                top
            } else {
                top + action_height + gap
            };
            if height > y + metrics.caption + gap + 2.0 * action_height {
                nodes.push(label(
                    appearance,
                    UiTextRole::Caption,
                    "choices-target",
                    &format!(
                        "{:?} choices for {}",
                        menu_kind,
                        menu_basis
                            .selected
                            .as_ref()
                            .map_or("the original entity", SceneEntityId::as_str)
                    ),
                    [viewport_x, y, viewport_width, metrics.caption],
                )?);
                y += metrics.caption + gap;
            }
            let viewport_height = (height - y - top).max(0.0);
            let choice_height = action_height + gap;
            let error_height = self
                .error
                .as_ref()
                .map_or(0.0, |_| 2.0 * metrics.body + gap);
            choice_limit = (error_height + count as f32 * choice_height - viewport_height).max(0.0);
            self.choice_scroll = self.choice_scroll.clamp(0.0, choice_limit);
            let range = visible_range(
                count,
                (self.choice_scroll - error_height).max(0.0),
                (viewport_height - (error_height - self.choice_scroll).max(0.0)).max(0.0),
                choice_height,
            );
            let text_limit = row_text_limit(range.len(), 1);
            let mut children = Vec::new();
            if let Some(error) = &self.error
                && self.choice_scroll < error_height
            {
                children.push(wrapped_label(
                    appearance,
                    "error",
                    error,
                    [0.0, -self.choice_scroll, viewport_width, 2.0 * metrics.body],
                )?);
            }
            for index in range {
                let control = projected_choice(
                    menu_kind,
                    index,
                    self.projection
                        .as_ref()
                        .expect("current projection captured"),
                    &scene.editor.inspector.entities,
                )
                .ok_or(OwnedHierarchyError::ChangedProjection)?;
                let (name, enabled) = choice(&control, &menu_basis);
                let name = presentation_text_with_limit(&name, text_limit);
                children.push(self.button(
                    control,
                    &name,
                    editing && enabled,
                    [
                        0.0,
                        error_height + index as f32 * choice_height - self.choice_scroll,
                        viewport_width,
                        action_height,
                    ],
                    false,
                    &mut buttons,
                )?);
            }
            if viewport_height > 0.0 {
                nodes.push(viewport(
                    viewport_x,
                    y,
                    viewport_width,
                    viewport_height,
                    children,
                ));
            }
        } else {
            let expanded = height
                >= 2.0 * inset
                    + metrics.title
                    + field_height
                    + 3.0 * action_height
                    + 6.0 * gap
                    + 2.0 * metrics.source_row;
            let mut y = top;
            if show_filter {
                if expanded {
                    nodes.push(label(
                        appearance,
                        UiTextRole::Title,
                        "heading",
                        "Hierarchy",
                        [inset, y, width, metrics.title],
                    )?);
                    y += metrics.title + gap;
                }
                nodes.push(self.filter.authored_node(
                    &self.root,
                    UiStyle::absolute(inset, y, width, field_height),
                    width,
                )?);
                y += field_height + gap;
                nodes.push(self.button(
                    Control::ClearFilter,
                    "Clear filter",
                    !self.filter.editor().text().is_empty() || self.filter.editor().is_composing(),
                    [inset, y, half, action_height],
                    false,
                    &mut buttons,
                )?);
                nodes.push(self.button(
                    Control::ShowSelection,
                    "Show selection",
                    selected.is_some(),
                    [inset + half + gap, y, half, action_height],
                    false,
                    &mut buttons,
                )?);
                y += action_height + gap;
            }
            let single_toolbar = height < y + 2.0 * action_height + 2.0 * gap + top + metrics.body;
            let columns = if single_toolbar { 5.0 } else { 3.0 };
            let third = ((width - (columns - 1.0) * gap) / columns).max(1.0);
            for (column, control, name, enabled) in [
                (0.0, Control::Root, "+ Root", editing),
                (1.0, Control::Child, "+ Child", editing && selected_exists),
                (
                    2.0,
                    Control::Remove,
                    "Remove Entity",
                    editing
                        && selected_exists
                        && basis
                            .as_ref()
                            .is_some_and(|basis| basis.removal_reason.is_none()),
                ),
            ] {
                nodes.push(self.button(
                    control,
                    name,
                    enabled,
                    [inset + column * (third + gap), y, third, action_height],
                    false,
                    &mut buttons,
                )?);
            }
            let placement = basis.as_ref().and_then(|basis| basis.placement.as_ref());
            let parent_y = if single_toolbar {
                y
            } else {
                y + action_height + gap
            };
            let parent_x = if single_toolbar {
                inset + 3.0 * (third + gap)
            } else {
                inset
            };
            let parent_width = if single_toolbar { third } else { half };
            nodes.push(self.button(
                Control::Parent,
                "Parent",
                editing && selected_exists,
                [parent_x, parent_y, parent_width, action_height],
                false,
                &mut buttons,
            )?);
            nodes.push(self.button(
                Control::Order,
                "Order",
                editing && selected_exists && placement.is_some(),
                [
                    parent_x + parent_width + gap,
                    parent_y,
                    parent_width,
                    action_height,
                ],
                false,
                &mut buttons,
            )?);
            y = parent_y + action_height + gap;
            let mut viewport_height = (height - y - top).max(0.0);
            let mut details = Vec::new();
            if expanded {
                details.push((
                    "source-kind",
                    "Source declarations; resolved hierarchy is unavailable in this view"
                        .to_owned(),
                    false,
                ));
                details.push(("placement", placement_intent_label(placement), false));
            } else {
                details.push((
                    "source-kind",
                    "Source declarations; authored placement intent".to_owned(),
                    false,
                ));
            }
            if let Some(reason) = basis.as_ref().and_then(|basis| basis.removal_reason) {
                details.push(("removal-unavailable", reason.to_owned(), false));
            }
            if busy {
                details.push((
                    "pending",
                    "Awaiting the exact Hierarchy action result".to_owned(),
                    false,
                ));
            }
            if let Some(error) = &self.error {
                details.push(("error", error.clone(), true));
            }
            if let Some(error) = &self.input_error {
                details.push(("input-error", error.clone(), true));
            }
            let mut children = Vec::new();
            if let Some(scene) = model
                .workspace()
                .active_scene
                .as_ref()
                .filter(|_| basis.is_some())
            {
                let projection = self
                    .projection
                    .as_mut()
                    .expect("current projection captured");
                projection.refilter(
                    &scene.editor.inspector.entities,
                    self.filter.editor().text(),
                    &mut self.projection_work,
                );
                let matches = projection.matching_count();
                let diagnostics = &scene.editor.inspector.diagnostics;
                let total = matches + diagnostics.iter().len();
                let count = format!(
                    "{matches} of {} source entities",
                    scene.editor.inspector.entities.len()
                );
                if viewport_height >= metrics.caption + gap + metrics.body {
                    nodes.push(label(
                        appearance,
                        UiTextRole::Caption,
                        "source-count",
                        &count,
                        [inset, y, width, metrics.caption],
                    )?);
                    y += metrics.caption + gap;
                    viewport_height = (height - y - top).max(0.0);
                } else {
                    details.insert(0, ("source-count", count, false));
                }
                let prefix_height = details
                    .iter()
                    .map(|(_, _, wrapped)| {
                        if *wrapped {
                            2.0 * metrics.body + gap
                        } else {
                            metrics.caption + gap
                        }
                    })
                    .sum::<f32>();
                if self.reveal_selection {
                    self.source_scroll = prefix_height
                        + projection.selected_index.unwrap_or(0) as f32 * metrics.source_row;
                    self.reveal_selection = false;
                }
                source_limit =
                    (prefix_height + total as f32 * metrics.source_row - viewport_height).max(0.0);
                self.source_scroll = self.source_scroll.clamp(0.0, source_limit);
                let range = visible_range(
                    total,
                    (self.source_scroll - prefix_height).max(0.0),
                    (viewport_height - (prefix_height - self.source_scroll).max(0.0)).max(0.0),
                    metrics.source_row,
                );
                let text_limit = row_text_limit(range.len(), 3);
                let visible_sources = projection.visible_sources(range.clone());
                for (offset, index) in visible_sources.into_iter().enumerate() {
                    let row = &scene.editor.inspector.entities[index];
                    let top = prefix_height + (range.start + offset) as f32 * metrics.source_row
                        - self.source_scroll;
                    let name = format!("Select {}", row.id.as_str());
                    children.push(self.button(
                        Control::Select(row.id.clone()),
                        &presentation_text_with_limit(&name, text_limit),
                        basis.as_ref().is_some_and(|basis| basis.navigation) && !busy,
                        [0.0, top, width, action_height],
                        Some(&row.id) == selected,
                        &mut buttons,
                    )?);
                    let metadata = format!(
                        "{}{}{} components{}",
                        row.name.as_deref().unwrap_or(""),
                        if row.name.is_some() { " / " } else { "" },
                        row.inspectable_component_count,
                        if row.has_prefab { ", prefab" } else { "" }
                    );
                    let identity = self.controls[&Control::Select(row.id.clone())].identity;
                    children.push(label(
                        appearance,
                        UiTextRole::Caption,
                        &format!("source.{identity}.metadata"),
                        &presentation_text_with_limit(&metadata, text_limit),
                        [0.0, top + action_height + gap, width, metrics.caption],
                    )?);
                    let placement = placement_intent_label(row.placement.as_ref());
                    children.push(label(
                        appearance,
                        UiTextRole::Caption,
                        &format!("source.{identity}.placement"),
                        &presentation_text_with_limit(&placement, text_limit),
                        [
                            0.0,
                            top + action_height + gap + metrics.caption,
                            width,
                            metrics.caption,
                        ],
                    )?);
                }
                let diagnostic_start = range.start.saturating_sub(matches);
                let diagnostic_end = range.end.saturating_sub(matches);
                for (offset, diagnostic) in diagnostics
                    .iter()
                    .skip(diagnostic_start)
                    .take(diagnostic_end.saturating_sub(diagnostic_start))
                    .enumerate()
                {
                    let top = prefix_height
                        + (matches + diagnostic_start + offset) as f32 * metrics.source_row
                        - self.source_scroll;
                    let text = format!(
                        "{:?} {}: {}",
                        diagnostic.severity(),
                        diagnostic.code().as_str(),
                        diagnostic.summary().as_str()
                    );
                    children.push(wrapped_label(
                        appearance,
                        &format!("diagnostic.{}", diagnostic_start + offset),
                        &presentation_text_with_limit(&text, text_limit),
                        [0.0, top, width, metrics.source_row - gap],
                    )?);
                }
                if total == 0 {
                    children.push(label(
                        appearance,
                        UiTextRole::Body,
                        "empty-source",
                        if self.filter.editor().text().is_empty() {
                            "No entities"
                        } else {
                            "No matching source entities"
                        },
                        [0.0, prefix_height - self.source_scroll, width, metrics.body],
                    )?);
                }
            } else {
                children.push(label(
                    appearance,
                    UiTextRole::Body,
                    "no-document",
                    "No active document",
                    [0.0, 0.0, width, metrics.body],
                )?);
            }
            let mut detail_y = -self.source_scroll;
            for (identity, text, wrapped) in details {
                let line_height = if wrapped {
                    2.0 * metrics.body
                } else {
                    metrics.caption
                };
                if detail_y + line_height > 0.0 && detail_y < viewport_height {
                    children.push(if wrapped {
                        wrapped_label(
                            appearance,
                            identity,
                            &text,
                            [0.0, detail_y, width, line_height],
                        )?
                    } else {
                        label(
                            appearance,
                            UiTextRole::Caption,
                            identity,
                            &text,
                            [0.0, detail_y, width, line_height],
                        )?
                    });
                }
                detail_y += line_height + gap;
            }
            if viewport_height > 0.0 {
                nodes.push(viewport(inset, y, width, viewport_height, children));
            }
        }
        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill().with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::solid(appearance.palette().panel))
        .with_children(nodes);
        let snapshot = UiAuthoringSnapshotBuilder::new(self.next_snapshot)
            .with_root(root)
            .build()?;
        self.work.record_snapshot(&snapshot);
        if self.frames.last().is_some_and(|previous| {
            previous.frame.environment() != environment
                || previous.theme != theme
                || previous.source_limit != source_limit
                || previous.choice_limit != choice_limit
        }) {
            self.invalidate_actions();
        }
        if let Some(previous) = self.frames.last_mut()
            && previous.snapshot.roots() == snapshot.roots()
            && previous.frame.environment() == environment
            && previous.theme == theme
            && previous.appearance == appearance
            && previous.basis == basis
            && Arc::ptr_eq(&previous.observation, &self.observation)
            && previous.source_limit == source_limit
            && previous.choice_limit == choice_limit
        {
            self.work.reused_frames = self.work.reused_frames.saturating_add(1);
            previous.model_revision = model.revision();
            previous.read_only = read_only;
            self.paint_dirty = false;
            return Ok(Arc::clone(&previous.frame));
        }
        let next_snapshot = self
            .next_snapshot
            .checked_add(1)
            .ok_or(OwnedHierarchyError::IdentityExhausted)?;
        let frame = self.compiler.compile(&snapshot, environment, theme)?;
        self.next_snapshot = next_snapshot;
        if !frame.root_outcomes().iter().any(|outcome| {
            outcome.key() == &self.root && outcome.status() == UiRootStatus::Published
        }) {
            self.cancel_input()?;
            self.controls.clear();
            return Err(OwnedHierarchyError::Unpublished);
        }
        let visible = frame
            .layouts()
            .iter()
            .filter(|layout| layout.visible())
            .filter_map(|layout| layout.key())
            .collect::<HashSet<_>>();
        let projected = buttons
            .values()
            .map(|button| &button.control)
            .collect::<HashSet<_>>();
        self.identities
            .retain(|control, _| projected.contains(control));
        buttons.retain(|key, _| visible.contains(key));
        let input = (show_filter && visible.contains(self.filter.key())).then(|| DisplayedInput {
            key: self.filter.key().clone(),
            scope: self.scope,
            width,
            display_text: self.filter.editor().display_text().to_owned(),
        });
        self.controls
            .retain(|_, control| buttons.contains_key(&control.key));
        if input.is_none() {
            self.filter.blur(&mut self.engine)?;
        }
        self.frames.push(CompiledHierarchy {
            model_revision: model.revision(),
            read_only,
            frame: Arc::clone(&frame),
            snapshot,
            theme,
            appearance,
            basis,
            observation: Arc::clone(&self.observation),
            viewport_observation: Arc::clone(&self.viewport_observation),
            buttons,
            input,
            source_limit,
            choice_limit,
        });
        if self.frames.len() > RETAINED_FRAMES {
            self.frames.remove(0);
        }
        self.paint_dirty = false;
        Ok(frame)
    }

    fn button(
        &mut self,
        action: Control,
        name: &str,
        enabled: bool,
        rect: [f32; 4],
        selected: bool,
        displayed: &mut HashMap<UiKey, DisplayedButton>,
    ) -> Result<UiNode, OwnedHierarchyError> {
        if !self.controls.contains_key(&action) {
            let identity = if let Some(identity) = self.identities.get(&action) {
                *identity
            } else {
                let identity = self.next_control;
                self.next_control = identity
                    .checked_add(1)
                    .ok_or(OwnedHierarchyError::IdentityExhausted)?;
                self.identities.insert(action.clone(), identity);
                identity
            };
            // Allocated to the typed action/source identity, never a visible list ordinal.
            let key = UiKey::scoped(
                self.root.clone(),
                UiLocalKey::runtime("nara.hierarchy.action", identity.to_string())?,
            )?;
            self.controls.insert(
                action.clone(),
                ButtonControl {
                    identity,
                    key,
                    state: UiButtonState::default(),
                },
            );
        }
        let control = &self.controls[&action];
        let button = UiButton::new(
            control.key.clone(),
            presentation_text(name),
            UiActionId::new("hierarchy.activate")?,
        )?
        .with_enabled(enabled);
        let variant = match action {
            Control::ConfirmRemoval => UiButtonVariant::Destructive,
            Control::Root | Control::Child => UiButtonVariant::Secondary,
            _ => UiButtonVariant::Quiet,
        };
        let mut recipe = self.appearance.button(
            &button,
            control.state,
            variant,
            UiControlSize::Small,
            selected,
        )?;
        if action == Control::Remove && enabled {
            recipe = recipe.with_text_brush(nara_ui_kernel::UiBrush::Solid(
                self.appearance.palette().danger_text,
            ));
        }
        let node = recipe.authored_node(
            &button,
            &self.root,
            UiStyle::absolute(rect[0], rect[1], rect[2], rect[3])
                .with_overflow(UiOverflow::Clip)
                .with_width(UiDimension::Px(rect[2])),
        )?;
        displayed.insert(
            control.key.clone(),
            DisplayedButton {
                control: action,
                button,
            },
        );
        Ok(node)
    }
}

fn presentation_text(text: &str) -> Cow<'_, str> {
    presentation_text_with_limit(text, MAX_PRESENTATION_BYTES)
}

fn row_text_limit(rows: usize, texts_per_row: usize) -> usize {
    (ROW_PRESENTATION_BYTES / rows.saturating_mul(texts_per_row).max(1))
        .clamp(1, MAX_PRESENTATION_BYTES)
}

fn presentation_text_with_limit(text: &str, maximum: usize) -> Cow<'_, str> {
    if text.len() <= maximum {
        return Cow::Borrowed(text);
    }
    let mut digest = DefaultHasher::new();
    text.hash(&mut digest);
    let digest = digest.finish();
    let marker = if maximum >= 64 {
        format!(" … [{} bytes, {digest:016x}] … ", text.len())
    } else if maximum <= 2 {
        "~".repeat(maximum)
    } else {
        let hex = format!("{digest:016x}");
        format!("~{}~", &hex[..(maximum - 2).min(hex.len())])
    };
    let retained = (maximum - marker.len()) / 2;
    let mut prefix_end = retained;
    while !text.is_char_boundary(prefix_end) {
        prefix_end -= 1;
    }
    let mut suffix_start = text.len() - retained;
    while !text.is_char_boundary(suffix_start) {
        suffix_start += 1;
    }
    Cow::Owned(format!(
        "{}{}{}",
        &text[..prefix_end],
        marker,
        &text[suffix_start..]
    ))
}

fn projected_choice(
    kind: MenuKind,
    index: usize,
    projection: &SourceProjection,
    rows: &[SceneInspectorEntityRow],
) -> Option<Control> {
    let ordinal = match (kind, index) {
        (MenuKind::Parent, 0) => return Some(Control::ParentRoot),
        (MenuKind::Order, 0) => return Some(Control::First),
        (MenuKind::Order, 1) => return Some(Control::Last),
        (MenuKind::Parent, index) => index - 1,
        (MenuKind::Order, index) => (index - 2) / 2,
    };
    let row = rows.get(projection.other_source(ordinal)?)?;
    Some(match kind {
        MenuKind::Parent => Control::ParentEntity(row.id.clone()),
        MenuKind::Order if index.is_multiple_of(2) => Control::Before(row.id.clone()),
        MenuKind::Order => Control::After(row.id.clone()),
    })
}

fn visible_range(
    total: usize,
    scroll: f32,
    height: f32,
    row_height: f32,
) -> std::ops::Range<usize> {
    if height <= 0.0 {
        return 0..0;
    }
    let start = ((scroll / row_height).floor() as usize).min(total);
    let end = (((scroll + height) / row_height).ceil() as usize).min(total);
    start..end
}

fn viewport(x: f32, y: f32, width: f32, height: f32, children: Vec<UiNode>) -> UiNode {
    UiNode::new(UiStyle::absolute(x, y, width, height).with_overflow(UiOverflow::Clip))
        .with_children(children)
}

fn label(
    appearance: UiAppearance,
    role: UiTextRole,
    identity: &str,
    text: &str,
    rect: [f32; 4],
) -> Result<UiNode, OwnedHierarchyError> {
    Ok(UiNode::new(
        UiStyle::absolute(rect[0], rect[1], rect[2], rect[3]).with_overflow(UiOverflow::Clip),
    )
    .with_key(UiLocalKey::runtime("nara.hierarchy.label", identity)?)
    .with_text(
        appearance
            .text(presentation_text(text), role, role == UiTextRole::Caption)?
            .with_overflow(UiTextOverflow::Clip),
    ))
}

fn wrapped_label(
    appearance: UiAppearance,
    identity: &str,
    text: &str,
    rect: [f32; 4],
) -> Result<UiNode, OwnedHierarchyError> {
    Ok(UiNode::new(
        UiStyle::absolute(rect[0], rect[1], rect[2], rect[3]).with_overflow(UiOverflow::Clip),
    )
    .with_key(UiLocalKey::runtime("nara.hierarchy.label", identity)?)
    .with_text(appearance.text(presentation_text(text), UiTextRole::Body, false)?))
}

fn line_box(layout: &nara_text::TextLayout) -> f32 {
    let (min, max) = layout
        .lines()
        .iter()
        .fold((0.0_f32, layout.height()), |(min, max), line| {
            (min.min(line.block_min()), max.max(line.block_max()))
        });
    max - min
}

fn reference_appearance(theme: UiTheme) -> Result<UiAppearance, UiAppearanceError> {
    let colors = theme.palette();
    UiAppearance::default().with_palette(UiAppearancePalette {
        window: colors.surface,
        panel: colors.panel,
        raised: colors.panel,
        input: colors.surface,
        border: colors.secondary_text,
        text: colors.primary_text,
        muted_text: colors.secondary_text,
        disabled_text: colors.disabled,
        accent: colors.accent,
        on_accent: colors.primary_text,
        selection: colors.accent,
        focus: colors.focus,
        danger: colors.danger,
        danger_text: colors.danger,
        positive: colors.positive,
        hover: colors.secondary_text,
        pressed: colors.focus,
    })
}

fn choice(control: &Control, basis: &HierarchyBasis) -> (String, bool) {
    let parent = basis.placement.as_ref().map(|placement| &placement.parent);
    let slot = basis.placement.as_ref().map(|placement| &placement.slot);
    match control {
        Control::ParentRoot => (
            "Parent: Root".into(),
            parent != Some(&ScenePlacementParent::Root),
        ),
        Control::ParentEntity(entity) => (
            format!("Parent entity: {}", entity.as_str()),
            parent
                != Some(&ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    entity.clone(),
                ))),
        ),
        Control::First => (
            "Order: First".into(),
            slot != Some(&ScenePlacementSlot::First),
        ),
        Control::Last => (
            "Order: Last".into(),
            slot != Some(&ScenePlacementSlot::Last),
        ),
        Control::Before(entity) => (
            format!("Order: Before {}", entity.as_str()),
            slot != Some(&ScenePlacementSlot::Before(PrefabEntityTarget::source(
                entity.clone(),
            ))),
        ),
        Control::After(entity) => (
            format!("Order: After {}", entity.as_str()),
            slot != Some(&ScenePlacementSlot::After(PrefabEntityTarget::source(
                entity.clone(),
            ))),
        ),
        _ => (String::new(), false),
    }
}
