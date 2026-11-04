use std::{
    borrow::Cow,
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

use nara::tooling::SceneInspectorEntityRow;
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiBrush, UiColorRole, UiDimension, UiLayer, UiLocalKey,
    UiNode, UiOverflow, UiRoot, UiRootStatus, UiStyle, UiText, UiTextOverflow, UiVisual,
};
use nara_ui_widgets::UiButtonVisualState;

use super::*;

const MAX_PRESENTATION_BYTES: usize = 128;
// Leave the rest of the shared frame's glyph allowance for chrome and the local filter.
const ROW_PRESENTATION_BYTES: usize = 2048;

impl OwnedHierarchy {
    pub(in crate::frontend::imgui) fn compile(
        &mut self,
        model: &EditorFrameModel,
        environment: UiEnvironment,
        theme: UiTheme,
        read_only: bool,
    ) -> Result<Arc<UiFrame>, OwnedHierarchyError> {
        let width = environment.logical_size().width() - 2.0 * INSET;
        let height = environment.logical_size().height();
        if width <= 16.0 || height <= 2.0 * INSET {
            return Err(OwnedHierarchyError::InvalidViewport);
        }
        let basis = HierarchyBasis::capture(model, read_only);
        let scope = basis.as_ref().map(HierarchyBasis::scope);
        if self.scope != scope {
            self.filter.blur(&mut self.engine)?;
            self.filter = UiTextField::new(
                self.root.clone(),
                "Filter entities",
                "",
                TEXT_SIZE,
                &mut self.engine,
            )?;
            self.scope = scope;
            self.source_scroll = 0.0;
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
        let half = (width - 6.0) / 2.0;
        let busy = self.queued || self.pending.is_some();
        let editing = basis.as_ref().is_some_and(|basis| basis.editing) && !busy;
        let selected = basis.as_ref().and_then(|basis| basis.selected.as_ref());
        let selected_exists = model
            .workspace()
            .active_scene
            .as_ref()
            .is_some_and(|scene| {
                scene
                    .editor
                    .inspector
                    .entities
                    .iter()
                    .any(|row| Some(&row.id) == selected)
            });
        let mut source_limit = 0.0;
        let mut choice_limit = 0.0;
        let show_filter = self.removal.is_none() && self.choices.is_none() && height >= 180.0;
        if !show_filter {
            self.filter.blur(&mut self.engine)?;
        }
        if let Some(removal) = self.removal.clone() {
            // Confirmation is a complete panel mode: its actions never depend on chrome scroll.
            let top = if height < 128.0 { 2.0 } else { INSET };
            let action_height = (height - 2.0 * top).min(30.0);
            let entity = removal
                .selected
                .as_ref()
                .map_or("the original entity", SceneEntityId::as_str);
            let unchanged = basis.as_ref() == Some(&removal);
            nodes.push(self.button(
                Control::ConfirmRemoval,
                "Confirm removal",
                editing && unchanged && removal.removal_reason.is_none(),
                [INSET, top, half, action_height],
                false,
                &mut buttons,
            )?);
            nodes.push(self.button(
                Control::CancelRemoval,
                "Cancel removal",
                true,
                [INSET + half + 6.0, top, half, action_height],
                false,
                &mut buttons,
            )?);
            let y = top + action_height + 6.0;
            let remaining = (height - y - top).max(0.0);
            if remaining > 0.0 {
                nodes.push(wrapped_label(
                    "removal-question",
                    &format!("Remove the source-local branch rooted at {entity}?"),
                    INSET,
                    y,
                    width,
                    remaining.min(44.0),
                )?);
            }
            let y = y + 48.0;
            if height > y + top {
                nodes.push(wrapped_label("removal-warning", if unchanged {
                "Removal is refused if the source-local hierarchy cannot be resolved"
            } else {
                "The original selection or source changed. Cancel this confirmation before retrying."
                }, INSET, y, width, (height - y - top).min(44.0))?);
            }
            if let Some(error) = &self.error
                && height > y + 48.0 + top
            {
                nodes.push(wrapped_label(
                    "error",
                    error,
                    INSET,
                    y + 48.0,
                    width,
                    height - y - 48.0 - top,
                )?);
            }
        } else if let Some(choices) = &self.choices {
            let menu_kind = choices.kind;
            let menu_basis = choices.basis.clone();
            let scene = model
                .workspace()
                .active_scene
                .as_ref()
                .ok_or(OwnedHierarchyError::ChangedProjection)?;
            let others = scene
                .editor
                .inspector
                .entities
                .iter()
                .filter(|row| Some(&row.id) != menu_basis.selected.as_ref());
            let count = others.clone().count();
            let count = match menu_kind {
                MenuKind::Parent => count + 1,
                MenuKind::Order => count * 2 + 2,
            };
            let top = if height < 128.0 { 2.0 } else { INSET };
            let action_height = (height - 2.0 * top).min(28.0);
            nodes.push(self.button(
                Control::CloseChoices,
                "Close choices",
                true,
                [INSET, top, width, action_height],
                false,
                &mut buttons,
            )?);
            let mut y = top + action_height + 4.0;
            if height > 160.0 {
                nodes.push(label(
                    "choices-target",
                    &format!(
                        "{:?} choices for {}",
                        menu_kind,
                        menu_basis
                            .selected
                            .as_ref()
                            .map_or("the original entity", SceneEntityId::as_str)
                    ),
                    INSET,
                    y,
                    width,
                    20.0,
                )?);
                y += 24.0;
            }
            if let Some(error) = &self.error
                && height > y + 100.0
            {
                nodes.push(wrapped_label("error", error, INSET, y, width, 44.0)?);
                y += 48.0;
            }
            let viewport_height = (height - y - top).max(0.0);
            choice_limit = (count as f32 * CHOICE_HEIGHT - viewport_height).max(0.0);
            self.choice_scroll = self.choice_scroll.clamp(0.0, choice_limit);
            let range = visible_range(count, self.choice_scroll, viewport_height, CHOICE_HEIGHT);
            let text_limit = row_text_limit(range.len(), 1);
            // The iterator describes source targets. Only this viewport slice creates widgets.
            let options: Box<dyn Iterator<Item = Control> + '_> = match menu_kind {
                MenuKind::Parent => Box::new(
                    std::iter::once(Control::ParentRoot)
                        .chain(others.map(|row| Control::ParentEntity(row.id.clone()))),
                ),
                MenuKind::Order => Box::new([Control::First, Control::Last].into_iter().chain(
                    others.flat_map(|row| {
                        [
                            Control::Before(row.id.clone()),
                            Control::After(row.id.clone()),
                        ]
                    }),
                )),
            };
            let options = options
                .skip(range.start)
                .take(range.len())
                .collect::<Vec<_>>();
            let mut children = Vec::new();
            for (offset, control) in options.into_iter().enumerate() {
                let (name, enabled) = choice(&control, &menu_basis);
                let name = presentation_text_with_limit(&name, text_limit);
                children.push(self.button(
                    control,
                    &name,
                    editing && enabled,
                    [
                        0.0,
                        (range.start + offset) as f32 * CHOICE_HEIGHT - self.choice_scroll,
                        width,
                        CHOICE_HEIGHT,
                    ],
                    false,
                    &mut buttons,
                )?);
            }
            if viewport_height > 0.0 {
                nodes.push(viewport(y, width, viewport_height, children));
            }
        } else {
            let compact = height < 360.0;
            let tiny = height < 180.0;
            if show_filter {
                let filter_y = if compact { INSET } else { 38.0 };
                if !compact {
                    nodes.push(label("heading", "Hierarchy", INSET, INSET, width, 22.0)?);
                }
                nodes.push(self.filter.authored_node(
                    &self.root,
                    UiStyle::absolute(INSET, filter_y, width, 34.0),
                    width,
                )?);
                let buttons_y = filter_y + 40.0;
                nodes.push(self.button(
                    Control::ClearFilter,
                    "Clear filter",
                    !self.filter.editor().text().is_empty() || self.filter.editor().is_composing(),
                    [INSET, buttons_y, half, 28.0],
                    false,
                    &mut buttons,
                )?);
                nodes.push(self.button(
                    Control::ShowSelection,
                    "Show selection",
                    selected.is_some(),
                    [INSET + half + 6.0, buttons_y, half, 28.0],
                    false,
                    &mut buttons,
                )?);
            }
            let action_height = if tiny {
                ((height - 8.0) / 3.0).clamp(1.0, 28.0)
            } else {
                30.0
            };
            let action_y = if tiny {
                2.0
            } else if compact {
                86.0
            } else {
                112.0
            };
            let third = (width - 12.0) / 3.0;
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
                    [
                        INSET + column * (third + 6.0),
                        action_y,
                        third,
                        action_height,
                    ],
                    false,
                    &mut buttons,
                )?);
            }
            let placement = basis.as_ref().and_then(|basis| basis.placement.as_ref());
            let parent_y = if tiny {
                action_y + action_height + 2.0
            } else if compact {
                122.0
            } else {
                196.0
            };
            nodes.push(self.button(
                Control::Parent,
                "Parent",
                editing && selected_exists,
                [INSET, parent_y, half, action_height],
                false,
                &mut buttons,
            )?);
            nodes.push(self.button(
                Control::Order,
                "Order",
                editing && selected_exists && placement.is_some(),
                [INSET + half + 6.0, parent_y, half, action_height],
                false,
                &mut buttons,
            )?);
            if !compact {
                nodes.push(label(
                    "source-kind",
                    "Source declarations; resolved hierarchy is unavailable in this view",
                    INSET,
                    148.0,
                    width,
                    20.0,
                )?);
                nodes.push(label(
                    "placement",
                    &authoring::placement_intent_label(placement),
                    INSET,
                    171.0,
                    width,
                    20.0,
                )?);
            }
            let mut y = parent_y + action_height + 6.0;
            if compact && height >= 240.0 {
                nodes.push(label(
                    "source-kind",
                    "Source declarations; authored placement intent",
                    INSET,
                    y,
                    width,
                    20.0,
                )?);
                y += 24.0;
            }
            if let Some(reason) = basis.as_ref().and_then(|basis| basis.removal_reason) {
                nodes.push(label("removal-unavailable", reason, INSET, y, width, 20.0)?);
                y += 24.0;
            }
            if busy {
                nodes.push(label(
                    "pending",
                    "Awaiting the exact Hierarchy action result",
                    INSET,
                    y,
                    width,
                    20.0,
                )?);
                y += 24.0;
            }
            if let Some(error) = &self.error {
                nodes.push(wrapped_label("error", error, INSET, y, width, 44.0)?);
                y += 48.0;
            }
            if let Some(error) = &self.input_error {
                nodes.push(wrapped_label("input-error", error, INSET, y, width, 40.0)?);
                y += 44.0;
            }
            if let Some(scene) = model
                .workspace()
                .active_scene
                .as_ref()
                .filter(|_| basis.is_some())
            {
                let query = self.filter.editor().text().to_lowercase();
                let matching = scene
                    .editor
                    .inspector
                    .entities
                    .iter()
                    .filter(|row| matches_filter(row, &query));
                let matches = matching.clone().count();
                let diagnostics = &scene.editor.inspector.diagnostics;
                let total = matches + diagnostics.iter().len();
                if !compact {
                    nodes.push(label(
                        "source-count",
                        &format!(
                            "{matches} of {} source entities",
                            scene.editor.inspector.entities.len()
                        ),
                        INSET,
                        y,
                        width,
                        20.0,
                    )?);
                    y += 24.0;
                }
                let viewport_height = (height - y - INSET).max(0.0);
                source_limit = (total as f32 * SOURCE_ROW_HEIGHT - viewport_height).max(0.0);
                self.source_scroll = self.source_scroll.clamp(0.0, source_limit);
                let range = visible_range(
                    total,
                    self.source_scroll,
                    viewport_height,
                    SOURCE_ROW_HEIGHT,
                );
                let text_limit = row_text_limit(range.len(), 3);
                let mut children = Vec::new();
                for (offset, row) in matching.skip(range.start).take(range.len()).enumerate() {
                    let top =
                        (range.start + offset) as f32 * SOURCE_ROW_HEIGHT - self.source_scroll;
                    let name = format!("Select {}", row.id.as_str());
                    children.push(self.button(
                        Control::Select(row.id.clone()),
                        &presentation_text_with_limit(&name, text_limit),
                        basis.as_ref().is_some_and(|basis| basis.navigation) && !busy,
                        [0.0, top, width, 26.0],
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
                        &format!("source.{identity}.metadata"),
                        &presentation_text_with_limit(&metadata, text_limit),
                        0.0,
                        top + 28.0,
                        width,
                        17.0,
                    )?);
                    let placement = authoring::placement_intent_label(row.placement.as_ref());
                    children.push(label(
                        &format!("source.{identity}.placement"),
                        &presentation_text_with_limit(&placement, text_limit),
                        0.0,
                        top + 46.0,
                        width,
                        20.0,
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
                    let top = (matches + diagnostic_start + offset) as f32 * SOURCE_ROW_HEIGHT
                        - self.source_scroll;
                    let text = format!(
                        "{:?} {}: {}",
                        diagnostic.severity(),
                        diagnostic.code().as_str(),
                        diagnostic.summary().as_str()
                    );
                    children.push(wrapped_label(
                        &format!("diagnostic.{}", diagnostic_start + offset),
                        &presentation_text_with_limit(&text, text_limit),
                        0.0,
                        top,
                        width,
                        SOURCE_ROW_HEIGHT,
                    )?);
                }
                if total == 0 {
                    children.push(label(
                        "empty-source",
                        if query.is_empty() {
                            "No entities"
                        } else {
                            "No matching source entities"
                        },
                        0.0,
                        0.0,
                        width,
                        20.0,
                    )?);
                }
                if viewport_height > 0.0 {
                    nodes.push(viewport(y, width, viewport_height, children));
                }
            } else {
                nodes.push(label(
                    "no-document",
                    "No active document",
                    INSET,
                    y,
                    width,
                    20.0,
                )?);
            }
        }
        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill().with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::role(UiColorRole::Panel))
        .with_children(nodes);
        let snapshot = UiAuthoringSnapshotBuilder::new(self.next_snapshot)
            .with_root(root)
            .build()?;
        if self.frames.last().is_some_and(|previous| {
            previous.frame.environment() != environment
                || previous.theme != theme
                || previous.source_limit != source_limit
                || previous.choice_limit != choice_limit
        }) {
            self.invalidate_actions();
        }
        if let Some(previous) = self.frames.last()
            && previous.snapshot.roots() == snapshot.roots()
            && previous.frame.environment() == environment
            && previous.theme == theme
            && previous.basis == basis
            && Arc::ptr_eq(&previous.observation, &self.observation)
            && previous.source_limit == source_limit
            && previous.choice_limit == choice_limit
        {
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
            frame: Arc::clone(&frame),
            snapshot,
            theme,
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
        let color = match control.state.visual_state(&button) {
            UiButtonVisualState::Disabled => UiColorRole::Disabled,
            UiButtonVisualState::Pressed | UiButtonVisualState::Focused => UiColorRole::Focus,
            UiButtonVisualState::Hovered => UiColorRole::SecondaryText,
            UiButtonVisualState::Idle if selected => UiColorRole::Focus,
            UiButtonVisualState::Idle => UiColorRole::Panel,
        };
        let node = button.authored_node(
            &self.root,
            UiStyle::absolute(rect[0], rect[1], rect[2], rect[3])
                .with_overflow(UiOverflow::Clip)
                .with_width(UiDimension::Px(rect[2])),
            UiVisual::role(color),
            UiBrush::Role(UiColorRole::PrimaryText),
            TEXT_SIZE,
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

fn matches_filter(row: &SceneInspectorEntityRow, query: &str) -> bool {
    query.is_empty()
        || row.id.as_str().to_lowercase().contains(query)
        || row
            .name
            .as_ref()
            .is_some_and(|name| name.to_lowercase().contains(query))
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

fn viewport(y: f32, width: f32, height: f32, children: Vec<UiNode>) -> UiNode {
    UiNode::new(UiStyle::absolute(INSET, y, width, height).with_overflow(UiOverflow::Clip))
        .with_children(children)
}

fn label(
    identity: &str,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<UiNode, OwnedHierarchyError> {
    Ok(
        UiNode::new(UiStyle::absolute(x, y, width, height).with_overflow(UiOverflow::Clip))
            .with_key(UiLocalKey::runtime("nara.hierarchy.label", identity)?)
            .with_text(
                UiText::new(presentation_text(text), TEXT_SIZE)?
                    .with_overflow(UiTextOverflow::Clip),
            ),
    )
}

fn wrapped_label(
    identity: &str,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<UiNode, OwnedHierarchyError> {
    Ok(
        UiNode::new(UiStyle::absolute(x, y, width, height).with_overflow(UiOverflow::Clip))
            .with_key(UiLocalKey::runtime("nara.hierarchy.label", identity)?)
            .with_text(UiText::new(presentation_text(text), TEXT_SIZE)?),
    )
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
